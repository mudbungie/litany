//! `litany stop <repo> <branch> [--stop-children]` — SIGTERM per ARCH §2.9.
//!
//! **Default: stop the one agent.** A bare `litany stop` signals the
//! process group of the single executor driving `<branch>` — its
//! provider adapter (`bz`) and cooperating tool subprocesses die with
//! it (§2.9 steps 1-2), in-flight HTTP dropped — with a 5-second flush
//! deadline before SIGKILL (the same cascade §4.4 pins for adapters and
//! §3.3 for tools, applied to the harness). The kernel pgid is scoped to
//! that **one** executor's own subprocesses: those are its limbs, not
//! agents. A still-running child on a descended branch is a *separate*
//! agent with its own pgid (each executor takes its own pgid at
//! startup, root and child alike, §2.9) and is **not** touched — it
//! outlives the parent and later deposits its result into the stopped
//! parent's inbox — a stop is an obituary, addressed by descent (§2.6).
//!
//! **`--stop-children`: walk the id namespace.** The agent→agent cascade
//! is opt-in. Descent is encoded in the hyphenated agent id (§2.3), so
//! the children (and all deeper descendants) of `<branch>` are exactly
//! the inbox directories prefixed `<branch>-` (single source of truth —
//! the flat id namespace *is* the tree, so one prefix scan covers every
//! depth; no separate recursion). The flag enumerates that prefix and
//! folds each descendant executor's pgid into the one SIGTERM sweep.
//!
//! No on-disk cancel marker is written: per §2.9 the on-disk
//! signature of a stopped branch is the latest step's `response.json`
//! closed (`IN_CLOSE_WRITE`, §3.5) without a terminal brazen `end`
//! event. The kernel produces that signature for free when the
//! harness terminates without flushing — same way crashes and
//! external kills are indistinguishable on disk per §2.9.
//!
//! Pid discovery derives from `/proc/<pid>/fd/*` symlink targets
//! against the agent's **inbox directory** — the executor lock's
//! `flock` home (§2.11), held for the whole step loop. The target is
//! `<workspace>/inbox/<branch>/` (plus each sibling `inbox/<branch>-*/`
//! under `--stop-children`). No sidecar pid file: the open lock fd is
//! the *is-anyone-driving* signal the §2.11 lock probe and §3.5
//! classification already read — and, unlike the `response.json`
//! model-call fd, it is open across tool execution and between-step
//! gaps too, so a stop lands whenever an executor is alive (§2.9).
//!
//! **A discovered pgid is vetted twice before anything is signalled**
//! (§2.9). [`discover`] refuses a pgid that is not its holder's own pid
//! — a settled executor is a group leader, and a non-leader reading is
//! the group the executor *inherited from its spawner*. [`vet_targets`]
//! then refuses any pgid this process itself belongs to. The two are
//! one hazard seen from both ends: `kill(-pgid, SIGTERM)` against an
//! unsettled reading fells the operator's shell job in production, and
//! did fell the coverage runner under `make check`.

use crate::prompt::inbox::INBOX_DIR;
use crate::prompt::notice::notice;
use crate::template::{GitRunner, RealGit};
use std::io;
use std::path::{Path, PathBuf};
use std::time::Duration;
use thiserror::Error;

pub mod cascade;
pub mod discover;
pub mod inspector;

#[cfg(test)]
mod tests;

pub use cascade::{RealSignaler, Signaler, cascade};
pub use discover::{PgidFinder, ProcFsFinder};
pub use inspector::{BranchInspector, GitInspector};

/// SIGTERM-to-SIGKILL grace pinned by ARCH §2.9 (mirrors §4.4 / §3.3).
/// Tests pass a sub-second deadline; production uses this constant.
pub const STOP_DEADLINE: Duration = Duration::from_secs(5);

/// Polling cadence while waiting for SIGTERM'd processes to exit.
/// Small enough that user stop feels instant, large enough that an
/// idle wait costs nothing measurable.
const POLL_INTERVAL: Duration = Duration::from_millis(50);

/// Every way [`run`] can fail. Idempotent paths (no lock holder found,
/// already-stopped) are `Ok(())`, not errors — `litany stop` is a
/// fire-and-forget operation, not a transactional one.
#[derive(Debug, Error)]
pub enum Error {
    #[error("branch {0:?} does not exist in repo")]
    BranchMissing(String),
    #[error(transparent)]
    Layout(#[from] crate::workspace::LayoutError),
    #[error("git {op}: {source}")]
    Git {
        op: &'static str,
        #[source]
        source: io::Error,
    },
    #[error("scan /proc: {0}")]
    Proc(#[source] io::Error),
    #[error(
        "refusing to signal process group {pgid}: it is this process's own group, \
         so the SIGTERM would fell whatever launched `litany stop` — an operator's \
         shell job, or a test runner — rather than an executor (ARCH §2.9). \
         Discovery resolved a pgid that no detached executor can legitimately own; \
         nothing was signalled."
    )]
    SelfGroup { pgid: i32 },
    #[error("walk inbox directory: {0}")]
    InboxWalk(#[source] io::Error),
}

/// Stop the harness driving `branch`; optionally its subagent subtree.
///
/// 1. Validate `agents/<branch>` exists in `<workspace>/repo.git`.
/// 2. Collect the inbox directories to signal (§2.11 lock homes):
///    `inbox/<branch>/` always, plus every `inbox/<branch>-*/`
///    descendant (hyphenated descent, §2.3) **iff** `stop_children` —
///    the opt-in agent→agent cascade. Default touches only the one
///    agent; a live child keeps running and revives the parent on its
///    later deposit (§2.9, §2.11).
/// 3. Resolve each lock holder's pgid via the supplied [`PgidFinder`].
/// 4. SIGTERM the unique pgid set, wait `deadline`, SIGKILL leftovers.
///
/// Idempotent: a stopped branch (no lock holder found) returns `Ok(())`.
// Four of the arguments are injected trait objects (inspector, finder,
// signaler, git) — a test seam, not a data clump; bundling them buys
// nothing and obscures the stub wiring the tests depend on.
#[allow(clippy::too_many_arguments)]
pub fn run(
    repo: &Path,
    branch: &str,
    stop_children: bool,
    inspector: &dyn BranchInspector,
    finder: &dyn PgidFinder,
    signaler: &dyn Signaler,
    deadline: Duration,
    git: &dyn GitRunner,
) -> Result<(), Error> {
    // The inspector takes the workspace path; it routes git against
    // the bare `<workspace>/repo.git` (ARCH §2.2).
    if !inspector
        .exists(repo, branch, git)
        .map_err(|source| Error::Git {
            op: "rev-parse --verify",
            source,
        })?
    {
        return Err(Error::BranchMissing(branch.to_owned()));
    }

    let inbox_dirs = collect_inbox_dirs(repo, branch, stop_children)?;
    let mut pgids = Vec::new();
    for dir in inbox_dirs {
        if let Some(pgid) = finder.find_holder_pgid(&dir).map_err(Error::Proc)? {
            pgids.push(pgid);
        }
    }
    pgids.sort();
    pgids.dedup();
    if pgids.is_empty() {
        return Ok(());
    }
    vet_targets(&pgids, own_pgid())?;

    cascade(&pgids, signaler, deadline, POLL_INTERVAL);
    Ok(())
}

/// This process's own process group. `litany stop` never makes itself a
/// group leader, so this is whatever launched it: an operator's shell
/// job, or the test runner under `make check`.
// SAFETY: `getpgrp` takes no arguments, reads only the caller's own
// kernel state, and cannot fail.
fn own_pgid() -> i32 {
    unsafe { libc::getpgrp() }
}

/// Belt-and-braces last stop before the cascade: refuse to signal a
/// group the stop process itself belongs to (ARCH §2.9).
///
/// Discovery already refuses a pgid that is not its holder's own pid,
/// so reaching here with `own` in the set means that invariant was
/// somehow satisfied by a group we are standing in — impossible for a
/// detached executor, and catastrophic if signalled: `kill(-own, ...)`
/// reaches the invoking shell's job (production) or the coverage
/// runner (`make check`), which is exactly the observed failure this
/// guard closes off. Refuse the whole sweep rather than filter: a stop
/// that resolved a bogus target has not established what it *would*
/// have hit, and a half-performed kill is worse than none.
fn vet_targets(pgids: &[i32], own: i32) -> Result<(), Error> {
    match pgids.iter().find(|&&pgid| pgid == own) {
        Some(&pgid) => Err(Error::SelfGroup { pgid }),
        None => Ok(()),
    }
}

/// CLI entry point for `litany stop` (ARCH §3.4 — kept in the lib so
/// the bin file stays under the 300-line code cap and the wiring
/// itself is unit-testable). `stop_children` is the `--stop-children`
/// flag (§2.9): `false` stops the one agent, `true` walks the id
/// namespace. Production builds use the default deps; tests exercise
/// [`run`] directly with stubs.
pub fn cli_run(repo: &Path, branch: &str, stop_children: bool) -> Result<(), Error> {
    crate::workspace::require(repo)?;
    run(
        repo,
        branch,
        stop_children,
        &GitInspector,
        &ProcFsFinder::default(),
        &RealSignaler,
        STOP_DEADLINE,
        &RealGit::new(),
    )
}

/// Promote the calling process to a process-group leader so the
/// §2.9 cascade (`kill(-pgid, SIGTERM)`) reaches this executor's own
/// provider adapter and tool subprocesses without escaping into the
/// invoking shell or UI's process group — and, symmetrically, without
/// reaching *out* to a sibling or parent executor. Called at the top
/// of **every** driver: `litany prompt` (root) and `litany dispatch`
/// (child re-entry) alike. The old no-setpgid-for-child-harnesses rule
/// is retired (§2.9): a child executor takes its own pgid like a root,
/// so a bare `litany stop` on a parent cannot cross the agent boundary
/// into a running child — that cascade is now the opt-in CLI-level id
/// namespace walk of `--stop-children`, not a kernel-group side effect.
pub fn become_pgid_leader() {
    // SAFETY: setpgid is async-signal-safe; (0, 0) means "this
    // process; new group with itself as leader". Idempotent when
    // the process is already a pgid leader (typical when invoked
    // from a shell with job control). The branch-table is fully
    // exercised by `become_pgid_leader_with` (closure-injected
    // syscall); this wrapper itself is a one-liner.
    become_pgid_leader_with(|| unsafe { libc::setpgid(0, 0) });
}

/// Inner core for [`become_pgid_leader`]: parameterized on the
/// `setpgid` syscall so a unit test can exercise both branches
/// without mutating the test runner's pgid.
fn become_pgid_leader_with(setpgid: impl FnOnce() -> libc::c_int) {
    let r = setpgid();
    if r != 0 {
        notice!("setpgid: {}", io::Error::last_os_error());
    }
}

/// The inbox directory `inbox/<branch>/` — the home of the agent's own
/// executor lock (§2.11) — plus, **iff** `stop_children`, every
/// `inbox/<branch>-*/` descendant (hyphenated descent per §2.3). The
/// branch name itself is the agent id; descended subagent conversations
/// have ids that prefix-match the parent's (`<conv>-<sub>`), and the
/// single `<branch>-` prefix scan matches every depth of the subtree —
/// the flat id namespace already encodes the tree, so no recursion is
/// needed. Default (`stop_children == false`) returns only the one
/// agent's inbox, leaving live children untouched (§2.9). Absent
/// `inbox/` (an agent spawned but whose executor has not yet opened a
/// lock) yields an empty set — a stop with nothing to signal,
/// idempotently `Ok(())`.
fn collect_inbox_dirs(
    repo: &Path,
    branch: &str,
    stop_children: bool,
) -> Result<Vec<PathBuf>, Error> {
    let inbox_root = repo.join(INBOX_DIR);
    let mut dirs = Vec::new();
    if !inbox_root.exists() {
        return Ok(dirs);
    }
    let prefix_dash = format!("{branch}-");
    let entries = std::fs::read_dir(&inbox_root).map_err(Error::InboxWalk)?;
    for entry in entries {
        let entry = entry.map_err(Error::InboxWalk)?;
        let name = entry.file_name();
        let Some(name_str) = name.to_str() else {
            continue;
        };
        let is_self = name_str == branch;
        let is_descendant = stop_children && name_str.starts_with(&prefix_dash);
        if !is_self && !is_descendant {
            continue;
        }
        dirs.push(entry.path());
    }
    Ok(dirs)
}
