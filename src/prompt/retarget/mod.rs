//! **Retarget** — the change of config lineage, and of the role an agent
//! resolves as (ARCH §2.2, §4.3, §3.4).
//!
//! Fork chooses the lineage, and resolution follows its current tip at
//! every step boundary (§2.2, bl-403b) — an operator who fixes an
//! expired model id sees every conversation on the lineage pick it up
//! at its next step, no retarget needed. What remains this landing's to
//! do is the *lineage* itself: moving an agent onto a different
//! `config/*` line, or settling one held on its fork commit because
//! diverged lineages both reach it.
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
//!
//! **The role rides the same landing** (bl-946c). An agent's role lives
//! in its dispatch commit subject ([`role`]), and this landing mints a
//! fresh dispatch commit — so *changing the role is changing what that
//! commit says*, which is one field of the mint rather than a second
//! mechanism. `litany retarget --role planner` writes the role mark
//! beside the config one ([`workspace::retarget`]); either mark alone is
//! a legal act, and an absent one reads as *unchanged*, so a role-only
//! retarget re-forks the branch onto the config commit it already
//! resolves and settles it as a planner. That is plan mode for a whole
//! running conversation with no `litany config` pass (§6 *Plan mode is a
//! lineage*), and accepting the plan is `--role worker` back.

mod base;
mod preflight;

pub use preflight::preflight;

use super::{Error, WORKER_ROLE, dispatch, fork_point, rebase_forward, role};
use crate::prompt::rebase_forward::{Replay, Replayed};
use crate::template::GitRunner;
use crate::workspace;
use std::path::Path;

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

/// The commit the agent already resolves — the followed answer (§2.2
/// *Fork chooses the lineage*, bl-403b): a target the agent's next step
/// would read anyway is a clean no-op, so under follow-the-tip a
/// retarget onto the agent's own advanced lineage head no-ops, and what
/// retargets is a change of *lineage* (or the healing of a diverged
/// one).
fn governing(workspace_dir: &Path, branch: &str, git: &dyn GitRunner) -> Result<String, Error> {
    workspace::current_config::current_config(workspace_dir, branch, git)
        .map(|resolved| resolved.commit().trim().to_string())
        .map_err(|source| Error::Git {
            op: "retarget followed config",
            source,
        })
}

/// The role the branch has committed — its own fact (§6), read from the
/// dispatch commit subject reachable from `start` ([`role::derive`]). A
/// root founded before bl-946c carries no role there, which is
/// [`WORKER_ROLE`], exactly as step resolution reads it.
fn committed_role(
    access: &Path,
    agent_id: &str,
    start: &str,
    git: &dyn GitRunner,
) -> Result<String, Error> {
    Ok(role::derive(access, start, agent_id, git)?.unwrap_or_else(|| WORKER_ROLE.to_string()))
}

/// Consume `agent_id`'s retarget marks, if it has either, against its
/// branch checked out at `worktree` (ARCH §2.2). `Ok(None)` — neither
/// mark — is every agent's ordinary state at every boundary, so the whole
/// feature costs an unmarked branch two ref reads.
///
/// **An absent mark means unchanged, never nothing.** A config mark alone
/// re-forks onto another lineage under the role the branch already
/// carries; a role mark alone re-forks onto the commit it already
/// resolves under a new role (bl-946c); both move both. There is no
/// third case, because [`consume`] fills each absence from the branch's
/// own committed fact.
///
/// **Both marks are consumed in every outcome.** Landed, declined or
/// no-op alike, the question has been answered; a surviving mark would
/// re-ask it at the next boundary, and a declined landing would
/// re-attempt a rebase that has already been recorded as refused.
pub fn land(
    workspace_dir: &Path,
    agent_id: &str,
    worktree: &Path,
    git: &dyn GitRunner,
) -> Result<Option<Outcome>, Error> {
    let target = workspace::retarget::read(workspace_dir, agent_id, git);
    let role = workspace::retarget::read_role(workspace_dir, agent_id, git);
    if target.is_none() && role.is_none() {
        return Ok(None);
    }
    let outcome = consume(
        workspace_dir,
        agent_id,
        worktree,
        target.as_deref(),
        role.as_deref(),
        git,
    );
    let err = |source| Error::Git {
        op: "retarget clear mark",
        source,
    };
    // `update-ref -d` on an absent ref succeeds, so both are cleared
    // unconditionally rather than each behind a test of its own.
    workspace::retarget::clear(workspace_dir, agent_id, git).map_err(err)?;
    workspace::retarget::clear_role(workspace_dir, agent_id, git).map_err(err)?;
    outcome.map(Some)
}

/// The landing proper, split from [`land`] so the mark is consumed on
/// every path out of it — including a failure, which has still answered
/// the mark and must not be re-attempted at every subsequent boundary.
fn consume(
    workspace_dir: &Path,
    agent_id: &str,
    worktree: &Path,
    marked_commit: Option<&str>,
    marked_role: Option<&str>,
    git: &dyn GitRunner,
) -> Result<Outcome, Error> {
    let branch = workspace::agent_ref(agent_id);
    let governing = governing(workspace_dir, &branch, git)?;
    // An absent mark reads as the state the branch is already in, so a
    // config mark naming the commit already governing, with no role mark
    // beside it, is the whole of the no-op — and it is answered here,
    // before the founding commit is read, because that is the one
    // question a branch answers without one.
    let target = marked_commit.unwrap_or(&governing);
    if target == governing && marked_role.is_none() {
        return Ok(Outcome::NoOp);
    }
    let dispatch_sha = role::founding_sha(worktree, &branch, agent_id, git)?.ok_or(Error::Git {
        op: "retarget dispatch commit",
        source: std::io::Error::other(format!(
            "no dispatch commit founds [{agent_id}] on {branch} — a retarget re-forks the \
             branch off its own founding commit (ARCH §2.2)"
        )),
    })?;
    // A role mark is the answer where it stands; without one the branch
    // keeps the role its own dispatch commit records (§4.3).
    let role = match marked_role {
        Some(role) => role.to_string(),
        None => committed_role(worktree, agent_id, &dispatch_sha, git)?,
    };
    let tools = base::granted(workspace_dir, target, &role, git)?;
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
