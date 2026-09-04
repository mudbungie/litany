//! Checkpoint trigger evaluation (ARCH §2.6, §2.7, §6).
//!
//! Compaction runs at **checkpoints** during a branch's execution. The
//! triggers are declared in the governing config's `workflow.yaml`
//! `compaction:` block (§6) — `every_n_commits`, `every_t_seconds`, or
//! the agent-elected `on_flush` — and are read **at the step boundary by
//! the executor**, which already holds the loaded workflow config (§6 hop
//! step 4). A branch with no configured trigger never compacts (§2.7).
//!
//! This module is the evaluation, kept **minimal and binding-shaped** so
//! it slots into the workflow-binding interpreter (§6) rather than
//! standing as a parallel path: [`due`] is a pure predicate over the
//! config and a [`CheckpointState`] the executor derives from disk, and
//! [`state`] is that derivation. When the interpreter evaluates the
//! `compaction:` block at a boundary and the `worker_flush` event, it
//! computes the same state and asks the same predicate; today's boundary
//! hook calls them directly.
//!
//! The **checkpoint commit `C`** is the branch tip at the boundary where
//! [`due`] fires — the commit the dispatched compactor forks off (§2.6).
//! "Since the last checkpoint" is derived from git, never stored
//! (`docs/PRINCIPLES.md` Single source of truth).
//!
//! # Three invariants on eligibility
//!
//! **The clock starts at the branch's own founding commit.** A branch is
//! forked off its parent's tip and inherits the parent's whole history
//! (§2.3 *Fork and inheritance*), so "commits on this branch" can never
//! mean "commits reachable from HEAD": a seconds-old child would read its
//! parent's hundred commits as its own and be instantly due. The one
//! commit that founds a branch is its **dispatch commit**, `dispatch:
//! <role> [<agent-id>]` for a child and `step 001: dispatch [<agent-id>]`
//! for a root ([`crate::prompt::dispatch::step_commit`]). One anchored
//! pattern matches both spellings exactly
//! ([`crate::prompt::role::founding_pattern`] — the single home of that
//! question), so the root is not a special case; matching the
//! `[<agent-id>]` tail alone would not do, because the executor's own
//! transcript commits end in it too and would answer as the founding
//! ([`crate::prompt::dispatch::transcript`], bl-89f7). A branch's
//! checkpoint reference is therefore the newest of {its dispatch commit,
//! its last compaction base}, and the root commit only when neither
//! exists ([`reference::origin`]).
//!
//! **A compactor is never compaction-eligible.** A compactor *is* the
//! compaction, not a subject of one (§2.7): compacting it would fork a
//! compactor off a compactor, whose own transcript is the compaction it
//! was dispatched to perform. The role is derived from the same founding
//! commit ([`crate::prompt::role::derive`] — the single authoritative
//! home for an agent's role), so the exclusion costs no new state.
//!
//! **A compaction already in flight is a checkpoint that has fired.**
//! The two above bound *which branches* may be compacted; this one
//! bounds *when* a branch that may be is due, and it is a case neither
//! of them reaches — a branch legitimately eligible, firing the same
//! checkpoint again because the answer to its last firing has not come
//! back. The whole argument, and the residual it leaves, is
//! [`inflight`]'s.
//!
//! Either of the first two alone stops the runaway cascade of bl-a9eb
//! (yog bl-ebbd); all three are stated because they are different facts.

mod inflight;
mod reference;
pub(in crate::prompt) mod tail;
mod usage;

use super::Error;
use crate::config::{CompactionConfig, CompactionTrigger};
use crate::prompt::role;
use crate::template::GitRunner;
use std::path::Path;

pub use usage::LastUsage;
// The clock's reference commit and its subject vocabulary live one level
// down ([`reference`]); the landing reads them through this path, so the
// split is invisible to every consumer.
pub(super) use reference::{BASE_SUBJECT_PREFIX, landing_subject_pattern, origin, root_of};

/// Branch state a checkpoint trigger is evaluated against (§6), derived
/// from disk by [`state`]. Every field is a live derivation, never a
/// stored counter (`docs/PRINCIPLES.md` Single source of truth).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CheckpointState {
    /// Commits on the branch since the last compaction base (or branch
    /// root). Drives `every_n_commits`.
    pub commits_since_checkpoint: u32,
    /// Wall-clock seconds since the last checkpoint commit's timestamp
    /// (or the branch root's). Drives `every_t_seconds`. Saturating at 0,
    /// so a checkpoint dated in the future never reads as negative time.
    pub seconds_since_checkpoint: u64,
    /// The agent elected a flush this boundary (§2.7 "the `flush` action
    /// the agent may call"). Drives `on_flush`; the flush is the
    /// agent-elected trigger, distinct from the config-clock triggers.
    pub flush_requested: bool,
    /// This branch is itself a compactor — its role, derived from its own
    /// dispatch commit ([`crate::prompt::role::derive`]), is
    /// [`super::COMPACTOR_ROLE`]. A compactor is not a member of the
    /// compaction-eligible set at any commit count, elapsed time, or
    /// elected flush (module docs, §2.7).
    pub is_compactor: bool,
    /// A compaction this branch dispatched has not come back — a
    /// compactor child of it carries no returned mark ([`inflight`]).
    /// The checkpoint it answers has already fired, so firing again
    /// buys a second pass over the same span that cannot land (module
    /// docs, §2.7).
    pub compaction_in_flight: bool,
    /// The provider's report on the branch's newest model entry — its
    /// prompt side and the model's context window, both as reported
    /// ([`usage`]). Drives `window_percent`. `None` before the branch's
    /// first model call.
    pub last_usage: Option<LastUsage>,
}

/// Whether a checkpoint is due this boundary (§2.6, §2.7) — the one home
/// of compaction eligibility. `None` config — no configured trigger —
/// never compacts (§2.7). Two facts about the branch answer ahead of the
/// config and under **every** trigger, the agent-elected flush included:
/// **a compactor is never eligible** (module docs: it is the compaction,
/// not a subject of one), and **a branch with a compaction in flight is
/// not due** (its checkpoint has already fired; a second pass over the
/// same span cannot land). Otherwise the trigger kind selects the
/// predicate; a `None`/`0` `n` (guarded out at config load, §6) is never
/// due, so a malformed config fails closed rather than compacting every
/// step.
///
/// **Fallible for one trigger only.** `window_percent` measures against
/// a number only the provider can state, so a branch whose last usage
/// carries no context window is *declined* here rather than answered
/// "not due" — the one outcome that would leave a configured trigger
/// silently dead ([`usage`], `docs/DESIGN_CONTEXT_ECONOMY.md` §5.1).
/// Every other trigger's answer is total, and the two suppressors above
/// answer ahead of all of them.
pub fn due(cfg: Option<&CompactionConfig>, state: &CheckpointState) -> Result<bool, Error> {
    if state.is_compactor || state.compaction_in_flight {
        return Ok(false);
    }
    let Some(cfg) = cfg else {
        return Ok(false);
    };
    let threshold = |v: u64| {
        cfg.intermediate
            .n
            .is_some_and(|n| n > 0 && v >= u64::from(n))
    };
    Ok(match cfg.intermediate.trigger {
        CompactionTrigger::EveryNCommits => threshold(u64::from(state.commits_since_checkpoint)),
        CompactionTrigger::EveryTSeconds => threshold(state.seconds_since_checkpoint),
        CompactionTrigger::OnFlush => state.flush_requested,
        CompactionTrigger::WindowPercent => {
            return usage::due(cfg.intermediate.n, state.last_usage.as_ref());
        }
    })
}

/// Derive [`CheckpointState`] for the agent `agent_id`, whose branch is
/// checked out at `worktree` (§6). `now_unix` is the current wall-clock in
/// Unix seconds, supplied by the caller so this stays a pure derivation
/// over its inputs (§6 binding-shaped); `flush_requested` is the
/// agent-elected input. The commit count and the checkpoint timestamp both
/// measure from [`origin`] — the branch's own founding commit or its last
/// compaction base, whichever is newer — so an inherited history is never
/// counted as this branch's own (module docs).
pub fn state(
    worktree: &Path,
    agent_id: &str,
    now_unix: u64,
    flush_requested: bool,
    git: &dyn GitRunner,
) -> Result<CheckpointState, Error> {
    let last = origin(worktree, "HEAD", agent_id, git)?;
    Ok(CheckpointState {
        commits_since_checkpoint: commits_since(worktree, last.as_deref(), git)?,
        seconds_since_checkpoint: now_unix.saturating_sub(checkpoint_time(worktree, &last, git)?),
        flush_requested,
        is_compactor: role::derive(worktree, "HEAD", agent_id, git)?.as_deref()
            == Some(super::COMPACTOR_ROLE),
        compaction_in_flight: inflight::compaction_in_flight(worktree, agent_id, git)?,
        last_usage: usage::last(worktree)?,
    })
}

/// Count commits on `HEAD` after `last` (exclusive), or the whole branch
/// when `last` is `None`.
fn commits_since(worktree: &Path, last: Option<&str>, git: &dyn GitRunner) -> Result<u32, Error> {
    let range = match last {
        Some(sha) => format!("{sha}..HEAD"),
        None => "HEAD".to_string(),
    };
    let out = git
        .run_capture(worktree, &["rev-list", "--count", &range])
        .map_err(|source| Error::Git {
            op: "checkpoint rev-list count",
            source,
        })?;
    Ok(out.trim().parse::<u32>().unwrap_or(0))
}

/// Committer Unix timestamp of the reference commit: the branch's
/// [`origin`] when one exists, else the branch's root commit — the point
/// elapsed time is measured from.
fn checkpoint_time(
    worktree: &Path,
    last: &Option<String>,
    git: &dyn GitRunner,
) -> Result<u64, Error> {
    let reference = match last {
        Some(sha) => sha.clone(),
        None => root_of(worktree, "HEAD", git)?,
    };
    let out = git
        .run_capture(worktree, &["log", "-n", "1", "--format=%ct", &reference])
        .map_err(|source| Error::Git {
            op: "checkpoint commit time",
            source,
        })?;
    Ok(out.trim().parse::<u64>().unwrap_or(0))
}

#[cfg(test)]
mod tests;
