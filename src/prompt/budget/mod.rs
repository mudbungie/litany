//! Whole-tree budget enforcement (ARCH §6 "Budgets (v0.7)").
//!
//! `workflow.yaml` declares `budgets: {max_total_tokens, max_wall_seconds,
//! max_depth}` (all optional; omitted → unbounded, and the shipped
//! template declares none of them — ARCH §6 "Nothing ships bounded", so
//! every check below is vacuous until an operator declares a limit).
//! The harness checks
//! them at every model-call boundary, *before* invoking the adapter
//! (`crate::prompt::dispatch::run_exchange`). Spend, wall, and depth are
//! all derived from disk at check time by [`derive`] — no running counter
//! is stored (PRINCIPLES "Single source of truth").
//!
//! **One live whole-tree check — no inheritance.** A budget is a
//! per-conversation-tree ceiling, and `steps/` is one shared tree at the
//! conv-repo root, written live by every conversation (root and every
//! subagent) and never merged (ARCH §2.2/§2.3/§2.6). So any driver — root
//! or subagent — derives the *whole tree's* live spend against the root
//! id ([`root_of`]) and checks it against the single frozen `workflow.yaml`
//! limit. Nothing is handed down at dispatch: the child reads the same
//! total the parent would, so there is no snapshot to freeze and no
//! parent-minus-child to double-count. Tokens and wall derive over the
//! whole tree; `max_depth` is positional and derives from the driver's own
//! branch name. (An optional per-subtree cap — a future `--token-cap`-style
//! knob checked against a subtree's own spend — is not built here.)
//!
//! **Exhaustion is an ordinary terminal state.** On exhaustion the
//! harness ceases the branch's step loop and writes
//! `refs/litany/budget-exhausted/<branch>` ([`mark_exhausted`]) — the
//! same git-native marking pattern as the §2.6-step-6 conflicted ref.
//! No new event type, no `response.json` marker — a ref plus a stop,
//! which deposits an obituary into the dispatcher's inbox (§2.6) like any
//! other terminal event (ARCH §6, §2.11).
//!
//! **The depth boundary is ARCH §6's** ("The depth boundary"), not this
//! module's: depth counts dispatches from the root agent at depth 0, and
//! `max_depth` is the deepest *allowed* depth, so exhaustion is strict —
//! `depth(branch) > max_depth`. [`check`] implements exactly that; the
//! off-by-one is pinned at the model-call boundary by
//! `prompt/tests/budget_depth_boundary.rs`.

pub mod derive;
#[cfg(test)]
mod tests;

use crate::config::Budgets;
use crate::template::GitRunner;
use std::path::Path;

/// Git-native marker ref for an exhausted conversation
/// (`refs/litany/budget-exhausted/<branch>`, ARCH §6 — mirrors the
/// §2.6-step-6 conflicted ref). The single home of the prefix; the
/// terminal budget-exhausted state it marks is surfaced as a result
/// deposit into the dispatcher's inbox (§2.6 obituary, §2.11) like any other.
pub const BUDGET_EXHAUSTED_REF_PREFIX: &str = "refs/litany/budget-exhausted/";

/// Which declared limit a conversation crossed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Axis {
    Tokens,
    Wall,
    Depth,
}

impl std::fmt::Display for Axis {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Axis::Tokens => "max_total_tokens",
            Axis::Wall => "max_wall_seconds",
            Axis::Depth => "max_depth",
        })
    }
}

/// The crossed limit and the derived actual that crossed it. Carried for
/// the operator-facing diagnostic only — the terminal state is the ref,
/// not this value (ARCH §6 "an ordinary terminal state").
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Exhausted {
    pub axis: Axis,
    pub limit: u64,
    pub actual: u64,
}

impl std::fmt::Display for Exhausted {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{} exhausted ({}/{})",
            self.axis, self.actual, self.limit
        )
    }
}

/// Evaluate the single frozen `budgets` against spend/wall/depth derived
/// live from disk. Tokens and wall are whole-tree consumables — derived
/// over [`root_of`]`(branch)` (the branch plus its entire descent, ARCH
/// §6) and exhausted at `actual >= limit`, so the driver stops *before* it
/// overspends. Depth is positional — derived from `branch` itself and
/// exhausted at `actual > limit` (ARCH §6 "The depth boundary": the
/// deepest *allowed* depth, so the root at depth 0 is never
/// depth-exhausted). Returns the first
/// crossed axis, or `None` when every declared limit still has headroom;
/// an unbounded axis (`None` limit) never triggers.
pub fn check(repo: &Path, branch: &str, budgets: &Budgets) -> Option<Exhausted> {
    if let Some(limit) = budgets.max_total_tokens {
        let actual = derive::spend(repo, root_of(branch));
        if actual >= limit {
            return Some(Exhausted {
                axis: Axis::Tokens,
                limit,
                actual,
            });
        }
    }
    if let Some(limit) = budgets.max_wall_seconds {
        let actual = derive::wall_seconds(repo, root_of(branch));
        if actual >= limit {
            return Some(Exhausted {
                axis: Axis::Wall,
                limit,
                actual,
            });
        }
    }
    if let Some(limit) = budgets.max_depth {
        let limit = u64::from(limit);
        let actual = u64::from(derive::depth(branch));
        if actual > limit {
            return Some(Exhausted {
                axis: Axis::Depth,
                limit,
                actual,
            });
        }
    }
    None
}

/// The root conversation id of a branch: its first two hyphen-delimited
/// tokens (`<ts>-<short>`, ARCH §2.2). Every dispatch appends
/// `-<ts>-<short>` (hyphenated descent), so the root is the prefix before
/// the second hyphen. Whole-tree spend/wall derive against this, since
/// [`derive`] sums a branch plus its entire descent — the root's descent
/// *is* the whole tree. A bare root id (at most one hyphen) is its own root.
fn root_of(branch: &str) -> &str {
    match branch.match_indices('-').nth(1) {
        Some((idx, _)) => &branch[..idx],
        None => branch,
    }
}

/// Write the budget-exhausted marker ref for the agent id `branch` at
/// its tip (`git update-ref refs/litany/budget-exhausted/<agent-id>
/// HEAD`), run inside `worktree` — whose checked-out branch *is*
/// `agents/<agent-id>` (§2.3), so `HEAD` is the tip with no ref-name
/// round trip. State lives in git, not a sidecar file (PRINCIPLES SSOT)
/// — the same pattern as the §2.6 conflicted ref, which is likewise
/// keyed by agent id.
pub fn mark_exhausted(worktree: &Path, branch: &str, git: &dyn GitRunner) -> std::io::Result<()> {
    let ref_name = format!("{BUDGET_EXHAUSTED_REF_PREFIX}{branch}");
    git.run(worktree, &["update-ref", ref_name.as_str(), "HEAD"])
}
