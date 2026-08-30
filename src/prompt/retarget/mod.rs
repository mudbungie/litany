//! **Retarget** — the exit from the config freeze (ARCH §2.2, §3.4).
//!
//! Fork is the freeze (§2.2): the config commit governing an agent is the
//! nearest `config/*` ancestor of its branch, derived from ancestry and
//! never stored, so a config edit after the fork governs nothing that
//! agent does. Without an exit, a running agent is welded to the config it
//! forked off for life — an operator who fixes an expired model id watches
//! the very next step dispatch the old one, and the only alternative is to
//! abandon a live conversation's whole history to change one pointer.
//!
//! **Retarget is a re-fork, and it is the compaction landing's own two
//! moves** (§2.6) — no merge appears anywhere, because §2.3's invariant is
//! unconditional and §2.6 left no merge in the system to imitate:
//!
//! 1. **The base** ([`base`]) — a *newly minted* dispatch commit on top of
//!    the target config commit, derived through the fork's own machinery
//!    rather than rebased. Everything config-shaped is re-derived there:
//!    the descriptor cut, the control-file removal, the pinned soul.
//! 2. **The replay** ([`crate::prompt::rebase_forward`]) — the agent's own
//!    post-dispatch commits land on the new base, and the branch moves to
//!    the replayed tip. Transcript entries are one immutable file each
//!    with monotonic names (§2.3), so this is conflict-free by the same
//!    construction compaction relies on, and the same stage-reading
//!    decline applies where it is violated.
//!
//! After it, `governing_config` — unchanged, still a pure ancestry query —
//! answers the target commit. No new stored fact anywhere.
//!
//! **The user act is a ref mark, so the single-writer rule is untouched.**
//! `litany retarget` writes `refs/litany/retarget/<agent-id>`
//! ([`crate::workspace::retarget`]); the agent's **own executor** consumes
//! it at the next `advance` step boundary, exactly where the compaction
//! landing runs. §2.3 holds verbatim: the branch still advances by one
//! writer, and that writer is still its executor.
//!
//! **Why this rewrite is legitimate** is §2.6's own argument, applied to
//! the other axis. A compactor's payload is the dispatching branch's own
//! *context*, rewritten on purpose — its payload, not its contamination.
//! A retarget's payload is the branch's own *policy*, re-forked on
//! purpose. Same polarity, same landing, same writer.
//!
//! **Timing is semantics, not compromise.** The mark takes effect at the
//! agent's next step, never mid-step: a config governs steps, and a
//! retarget is in practice followed by a message, which *is* that next
//! step.

mod base;

use super::{Error, WORKER_ROLE, dispatch, fork_point, rebase_forward, role};
use crate::prompt::rebase_forward::{Replay, Replayed};
use crate::template::GitRunner;
use crate::workspace;
use std::path::Path;

/// Why a retarget needed the agent to exist, for the shared
/// [`workspace::require_agent`] decline (§2.3).
const REASON: &str = "a retarget re-forks a running agent off another config commit (ARCH §2.2)";

/// What consuming a retarget mark did to the branch.
#[derive(Debug, PartialEq, Eq)]
pub enum Outcome {
    /// The re-derived dispatch commit landed on the target config commit
    /// and every commit after the old one replayed on top. The branch's
    /// governing config *is* the target from this step on.
    Landed,
    /// The mark named the commit already governing the agent. A clean
    /// no-op, not an error: the operator asked for a state the branch is
    /// already in, and the general path with empty inputs answers it.
    NoOp,
    /// Git had to write conflict markers during the replay — the landing
    /// is aborted, the branch restored bit-for-bit, and
    /// `refs/litany/conflicted/<agent-id>` marked at the branch's own tip
    /// (§2.6 decline). Carries the offending paths for the operator line.
    Conflicted(Vec<String>),
}

/// Everything `litany retarget` refuses **before** the mark is written
/// (§3.4), returning the target config commit — or `None` when that commit
/// already governs the agent, which is a clean no-op rather than an error.
/// Nothing here writes, so a refusal leaves no debris at all: the same
/// validity-before-fork discipline the §6 budget gate and the §3.3
/// descriptor check hold to at every fork.
///
/// The checks are the ones a fork would run anyway, asked of the target:
/// the workspace and the agent exist, the config lineage exists, and the
/// agent's role — its own committed fact (§6) — is granted only tools the
/// target config describes ([`dispatch::require_described`]). What is
/// deliberately *not* checked is the tree: a retarget never inspects what
/// the branch has been doing, because the freeze it lifts is about policy.
pub fn preflight(
    workspace_dir: &Path,
    agent_id: &str,
    config_name: &str,
    git: &dyn GitRunner,
) -> Result<Option<String>, Error> {
    workspace::require(workspace_dir)?;
    workspace::require_agent(workspace_dir, agent_id, REASON, git)?;
    workspace::require_lineage(workspace_dir, config_name, git)
        .map_err(|e| Error::from(fork_point::Error::from(e)))?;
    let repo = workspace::repo_git(workspace_dir);
    let branch = workspace::agent_ref(agent_id);
    let spec = format!("{}^{{commit}}", workspace::config_ref(config_name));
    let target = git
        .run_capture(&repo, &["rev-parse", &spec])
        .map_err(|source| Error::Git {
            op: "retarget resolve target",
            source,
        })?
        .trim()
        .to_string();
    if governing(workspace_dir, &branch, git)? == target {
        return Ok(None);
    }
    let (role, tools) = grant_of(workspace_dir, &repo, agent_id, &branch, &target, git)?;
    dispatch::require_described(
        &repo,
        &dispatch::Grant {
            role: &role,
            tools: &tools,
            config_commit: &target,
        },
        git,
    )?;
    Ok(Some(target))
}

/// The role the branch committed and the `tools:` grant the **target**
/// config declares for it — read once here so the pre-flight validates
/// exactly the grant the base then forks with (§3.3). A root's dispatch
/// subject carries no role, which is the worker default, exactly as step
/// resolution reads it.
fn grant_of(
    workspace_dir: &Path,
    access: &Path,
    agent_id: &str,
    start: &str,
    target: &str,
    git: &dyn GitRunner,
) -> Result<(String, Vec<String>), Error> {
    let role =
        role::derive(access, start, agent_id, git)?.unwrap_or_else(|| WORKER_ROLE.to_string());
    let tools = base::granted(workspace_dir, target, &role, git)?;
    Ok((role, tools))
}

/// The agent's governing config commit (§2.2) — the pure ancestry query,
/// unchanged by any of this.
fn governing(workspace_dir: &Path, branch: &str, git: &dyn GitRunner) -> Result<String, Error> {
    workspace::governing_config(workspace_dir, branch, git)
        .map(|sha| sha.trim().to_string())
        .map_err(|source| Error::Git {
            op: "retarget governing config",
            source,
        })
}

/// Consume `agent_id`'s retarget mark, if it has one, against its branch
/// checked out at `worktree` (ARCH §2.2). `Ok(None)` — no mark — is every
/// agent's ordinary state at every boundary, so the whole feature costs an
/// unmarked branch one ref read.
///
/// **The mark is consumed in every outcome.** Landed, declined or no-op
/// alike, the question has been answered; a surviving mark would re-ask it
/// at the next boundary, and a declined landing would re-attempt a rebase
/// that has already been recorded as refused.
pub fn land(
    workspace_dir: &Path,
    agent_id: &str,
    worktree: &Path,
    git: &dyn GitRunner,
) -> Result<Option<Outcome>, Error> {
    let Some(target) = workspace::retarget::read(workspace_dir, agent_id, git) else {
        return Ok(None);
    };
    let outcome = consume(workspace_dir, agent_id, worktree, &target, git);
    workspace::retarget::clear(workspace_dir, agent_id, git).map_err(|source| Error::Git {
        op: "retarget clear mark",
        source,
    })?;
    outcome.map(Some)
}

/// The landing proper, split from [`land`] so the mark is consumed on
/// every path out of it — including a failure, which has still answered
/// the mark and must not be re-attempted at every subsequent boundary.
fn consume(
    workspace_dir: &Path,
    agent_id: &str,
    worktree: &Path,
    target: &str,
    git: &dyn GitRunner,
) -> Result<Outcome, Error> {
    let branch = workspace::agent_ref(agent_id);
    if governing(workspace_dir, &branch, git)? == target {
        return Ok(Outcome::NoOp);
    }
    let dispatch_sha = role::founding_sha(worktree, &branch, agent_id, git)?.ok_or(Error::Git {
        op: "retarget dispatch commit",
        source: std::io::Error::other(format!(
            "no dispatch commit founds [{agent_id}] on {branch} — a retarget re-forks the \
             branch off its own founding commit (ARCH §2.2)"
        )),
    })?;
    let (role, tools) = grant_of(
        workspace_dir,
        worktree,
        agent_id,
        &dispatch_sha,
        target,
        git,
    )?;
    let base = base::commit(
        workspace_dir,
        worktree,
        agent_id,
        &dispatch_sha,
        &dispatch::Grant {
            role: &role,
            tools: &tools,
            config_commit: target,
        },
        git,
    )?;
    // The replay is the shared rebase-forward move (§2.6): a decline marks
    // the agent's *own* ref, because that is where every byte of the
    // branch is — a retarget has no second branch to preserve.
    let replayed = rebase_forward::run(
        worktree,
        &Replay {
            branch_id: agent_id,
            point: &dispatch_sha,
            base: &base,
            mark_id: agent_id,
            mark_at: &branch,
        },
        git,
    )?;
    Ok(match replayed {
        Replayed::Landed => Outcome::Landed,
        Replayed::Conflicted(paths) => Outcome::Conflicted(paths),
    })
}

#[cfg(test)]
pub(crate) mod tests;
