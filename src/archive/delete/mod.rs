//! Agent removal — the retention half of ARCH §9.2 (*Retention and GC*),
//! behind the operator verb `litany delete`.
//!
//! §9.2 already says what removal *is*: "Deleting an expired or archived
//! agent is deleting its branches; `git gc` prunes whatever objects
//! nothing else reaches … The `steps/` and `inbox/` slices are plain
//! directories, removed with the branches." This module performs exactly
//! that, over the one subtree the archival unit is cut on (`<id>` plus
//! its `<id>-*` hyphen-descendants, §2.3) — so [`super::bundle`]
//! composes in front of it and **bundle-then-delete is the archive
//! path**: two verbs the caller sequences, with no `--archive` flag on
//! either (§9.2 "bundle-and-delete or delete outright").
//!
//! **The target set is the union of the id's homes, not the ref list.**
//! An agent is five things (§2.2, §2.3, §2.11): an `agents/<id>` ref, a
//! worktree at `agents/<id>/`, a `steps/<id>/` record tree, an
//! `inbox/<id>/` mailbox, and any `refs/litany/<kind>/<id>` marks. The
//! subtree is derived by asking *all five* who is present, so a delete
//! that died half-way leaves a state the next run completes: whatever
//! survived is still enumerated, and a run against an agent nothing
//! remembers is a quiet success over an empty set — convergence as the
//! general path with empty inputs, not a resume mode.
//!
//! **Two refusals, both fail-closed.** A bare delete of an agent with
//! descendants is declined naming them (`--children` is the explicit
//! subtree request, mirroring `stop --stop-children`, §2.9), and a
//! delete of an agent whose executor holds the §2.11 lock is declined
//! naming the lock — reaping the substrate beneath a running driver is a
//! race with a live process. Both are checked over the whole target set
//! before anything is removed.
//!
//! **`--dry-run` is the plan.** The verb's product is the same
//! [`DeleteReport`] either way — what dies, named — so a frontend's
//! confirmation dialog reads the identical sentence the receipt does
//! (§3.5: the frontend enumerates what a destructive verb takes).

use crate::prompt::inbox::INBOX_DIR;
use crate::prompt::step::STEPS_DIR;
use crate::template::GitRunner;
use crate::workspace::{self, AGENTS_DIR};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

mod subtree;

use subtree::{mark_refs, pending, require_quiescent, subtree};

/// The per-agent directory namespaces, all three id-keyed (§2.2): the
/// worktree parent and the two workspace-root slices. Named from their
/// owning modules' constants, so a rename travels.
const DIRS: [&str; 3] = [AGENTS_DIR, STEPS_DIR, INBOX_DIR];

/// What a delete removed — or, under `--dry-run`, would remove. The
/// verb's one stdout product (§3.4), and the census a caller's
/// confirmation enumerates: the descendants are *named* (their count is
/// `len`, never a second field) and the pending deposits are counted,
/// because those are the two things that die beyond the row the operator
/// is pointing at.
#[derive(Debug)]
pub struct DeleteReport {
    /// The subtree root's id, as asked for.
    pub agent: String,
    /// The `<id>-*` hyphen-descendants that die with it (§2.3), sorted.
    pub descendants: Vec<String>,
    /// Undelivered deposits across the subtree's inboxes (§2.11) — mail
    /// addressed *to* these agents, which dies with them. A deposit one
    /// of them *sent* already lives in the recipient's inbox and is
    /// untouched.
    pub pending_deposits: usize,
    /// Did this run remove it? `false` is the `--dry-run` plan.
    pub removed: bool,
}

impl std::fmt::Display for DeleteReport {
    /// One line, in `litany scan`'s voice: a leading verb phrase naming
    /// the mood and the agent, then `; `-separated `key: count (names)`
    /// fields.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mood = match self.removed {
            true => "deleted",
            false => "would delete",
        };
        let named = match self.descendants.is_empty() {
            true => String::new(),
            false => format!(" ({})", self.descendants.join(", ")),
        };
        write!(
            f,
            "{mood} {}; descendants: {}{named}; pending deposits: {}",
            self.agent,
            self.descendants.len(),
            self.pending_deposits
        )
    }
}

/// Every way [`delete`] can fail.
#[derive(Debug, thiserror::Error)]
pub enum DeleteError {
    /// Layout guard decline (§10): not a workspace, or the retired layout.
    #[error(transparent)]
    Layout(#[from] workspace::LayoutError),
    #[error("I/O error: {0}")]
    Io(#[from] io::Error),
    #[error("git {op}: {source}")]
    Git {
        op: &'static str,
        #[source]
        source: io::Error,
    },
    /// The bare form met a subtree (§2.3 descent): declined naming the
    /// descendants, since deleting them is a request nobody made.
    #[error(
        "agent {id:?} has {} descendant(s) — {}; deleting them is not implied, \
         pass --children to remove the whole subtree (ARCH §2.3, §9.2)",
        .descendants.len(),
        crate::name::pool(.descendants)
    )]
    HasDescendants {
        id: String,
        descendants: Vec<String>,
    },
    /// An executor holds the §2.11 lock over a target: never reap a live
    /// driver.
    #[error(
        "agent {id:?} is being driven — an executor holds its lock at {lock} \
         (ARCH §2.11); stop it first (`litany stop`) and delete once it is quiescent"
    )]
    Driven { id: String, lock: PathBuf },
    #[error("probe executor lock at {path}: {source}")]
    Probe {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
}

/// Remove the agent `agent_id` from `ws` (§9.2). `children` extends the
/// act to the whole descent subtree; `dry_run` computes the census and
/// removes nothing. Returns what died (or would).
///
/// The layout is guarded first ([`workspace::require`], §10), like every
/// verb. Existence is deliberately *not* guarded: the five id-taking
/// verbs decline an absent agent because their act would silently do
/// nothing, and delete's postcondition — this id has no state — an absent
/// agent already satisfies. Declining it would make crash recovery a
/// special case (§9.2 note in the module docs).
pub fn delete(
    ws: &Path,
    agent_id: &str,
    children: bool,
    dry_run: bool,
    git: &dyn GitRunner,
) -> Result<DeleteReport, DeleteError> {
    workspace::require(ws)?;
    let repo = workspace::repo_git(ws);
    let marks = mark_refs(&repo, git)?;
    let targets = subtree(ws, agent_id, &marks, git)?;
    let descendants: Vec<String> = targets
        .iter()
        .filter(|id| *id != agent_id)
        .cloned()
        .collect();
    if !children && !descendants.is_empty() {
        return Err(DeleteError::HasDescendants {
            id: agent_id.to_owned(),
            descendants,
        });
    }
    let mut pending_deposits = 0;
    for id in &targets {
        require_quiescent(ws, id)?;
        pending_deposits += pending(ws, id)?;
    }
    let report = DeleteReport {
        agent: agent_id.to_owned(),
        descendants,
        pending_deposits,
        removed: !dry_run,
    };
    if dry_run {
        return Ok(report);
    }
    for id in &targets {
        for dir in DIRS {
            remove_dir(&ws.join(dir).join(id))?;
        }
    }
    // The worktrees are gone from disk; this drops their administrative
    // entries, which is what frees the branch refs for deletion.
    run(git, &repo, &["worktree", "prune"], "worktree prune")?;
    for spec in ref_specs(&targets, &marks) {
        run(git, &repo, &["update-ref", "-d", &spec], "update-ref -d")?;
    }
    Ok(report)
}

/// The refs a delete of `targets` removes: each id's agent branch and
/// its **staged proposal** if it has one, plus every mark that names one
/// of them. An id is a single path component (§2.3), so a mark belongs
/// to a target exactly when it ends in that component.
///
/// A reviewer's `proposal/<id>` (`docs/DESIGN_LEARNING_LOOP.md` §3) is
/// listed unconditionally, exactly as the agent branch is: `update-ref
/// -d` on a ref that is not there is already the postcondition, and an
/// existence query would be a second answer to a question the delete
/// does not need asked. Reaping it with its reviewer is the same rule
/// the marks follow — a proposal nobody can now read the reasoning of
/// is debris — and rejecting it first is the operator's ordinary route
/// (`litany proposal <id> --reject`).
fn ref_specs(targets: &[String], marks: &[String]) -> Vec<String> {
    let mut specs: Vec<String> = targets
        .iter()
        .flat_map(|id| {
            [
                format!("refs/heads/{}", workspace::agent_ref(id)),
                format!("refs/heads/{}", workspace::proposal::proposal_ref(id)),
            ]
        })
        .collect();
    specs.extend(
        marks
            .iter()
            .filter(|r| targets.iter().any(|id| r.ends_with(&format!("/{id}"))))
            .cloned(),
    );
    specs
}

/// Remove a directory tree; an absent one is already the postcondition.
fn remove_dir(path: &Path) -> io::Result<()> {
    match fs::remove_dir_all(path) {
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(()),
        other => other,
    }
}

/// Run a fire-and-forget git op, tagging failures with `op`.
fn run(
    git: &dyn GitRunner,
    repo: &Path,
    args: &[&str],
    op: &'static str,
) -> Result<(), DeleteError> {
    git.run(repo, args)
        .map_err(|source| DeleteError::Git { op, source })
}
