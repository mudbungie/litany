//! The **reviewer** (`docs/DESIGN_LEARNING_LOOP.md` §2, ARCH §2.7) — the
//! role the checkpoint forks *beside* the compactor, off the same
//! compaction point.
//!
//! It is an ordinary child in every respect the harness has a mechanism
//! for: the same fork, the same dispatch commit, the same grant gate,
//! the same budget gate, the same front-door launch. Three facts are its
//! own, and each is keyed on the role rather than on a new mechanism:
//!
//! - it is dispatched by a `worker_flush: dispatch(reviewer)` binding
//!   ([`crate::prompt::dispatch::run_flush`]), so a config with no such
//!   binding never forks one — the off switch is a config edit;
//! - its dispatch commit **keeps the inherited dialog**, the third
//!   principled keeper beside the compactor and the fork-back-in root
//!   (ARCH §2.2, [`crate::prompt::dispatch::prune_inherited_dialog`]):
//!   the span about to be squashed is its subject;
//! - its dispatch commit **checks out the followed config commit's
//!   workspace skills and the durable-facts document**
//!   ([`crate::prompt::dispatch::step_commit`]'s reviewer read), so a
//!   fresh read precedes every write by construction.
//!
//! Its edits land nowhere: `stage_proposal` (§3 there, bl-5b62) consumes
//! the return into one commit on `proposal/<reviewer-id>`, which an
//! operator accepts or rejects. Until that action ships, the §6
//! interpreter declines it loudly (bl-30fe) — a reviewer that runs and
//! proposes still writes no lineage.

use super::{Error, subagent};
use std::path::Path;

/// Role name of the reviewer child (`docs/DESIGN_LEARNING_LOOP.md` §2).
/// Its soul is `souls/reviewer.md` in the governing config commit and its
/// grant — `[read_file, apply_patch]`, its whole confinement — is that
/// commit's `providers.yaml` row.
pub(crate) const REVIEWER_ROLE: &str = "reviewer";

/// Boilerplate goal handed to a reviewer at dispatch, read off the
/// **dispatching branch's worktree** so the reviewer's own goal can quote
/// that branch's goal verbatim — the shape [`super::compactor::compactor_goal`]
/// established, and for the same reason: the dispatching branch's goal has
/// no other route into a child whose own `goal.md` is this text (§2.8).
///
/// Short by design. What a reviewer looks for, what it may edit and how
/// it must answer are its **soul's** (`template/souls/reviewer.md`,
/// bl-30fe) — policy in config, not in code. What only the harness knows
/// is what this text carries: which branch, and what is in the tree it
/// was handed.
pub(crate) fn reviewer_goal(parent_worktree: &Path, parent_branch: &str) -> Result<String, Error> {
    let parent_goal = std::fs::read_to_string(parent_worktree.join(subagent::GOAL_FILE))?;
    Ok(format!(
        "You are the reviewer for branch `{parent_branch}`.\n\
         \n\
         In your context is that branch's transcript and its prior summaries\n\
         under `summary/`, up to the compaction point you were forked off. The\n\
         compactor forked beside you is about to squash that span out of the\n\
         branch's context, so what you do not carry out of it now stops being\n\
         inspectable — review before it is forgotten.\n\
         \n\
         Your tree also carries this workspace's own skills under `skills/`,\n\
         checked out fresh from the config commit that governs the branch, and\n\
         the workspace's durable facts document where the lineage carries one.\n\
         Those are what you may edit, and your edits do not land: they are\n\
         staged as a proposal an operator reads, accepts or rejects. An empty\n\
         proposal is the expected outcome and costs nobody anything.\n\
         \n\
         Judge what is worth outliving the span against the dispatching\n\
         branch's own goal, not your own preferences:\n\
         \n\
         <dispatching-branch-goal>\n\
         {parent_goal}\n\
         </dispatching-branch-goal>\n"
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_goal_names_the_branch_and_quotes_its_goal() {
        let dir = tempfile::TempDir::new().unwrap();
        std::fs::write(dir.path().join("goal.md"), "ship the thing\n").unwrap();
        let g = reviewer_goal(dir.path(), "20260101-p1").unwrap();
        assert!(g.contains("reviewer for branch `20260101-p1`"), "{g}");
        assert!(
            g.contains("<dispatching-branch-goal>\nship the thing\n\n</dispatching-branch-goal>"),
            "{g}"
        );
    }

    #[test]
    fn a_dispatching_branch_with_no_goal_declines() {
        // The same decline `compactor_goal` gives: a branch with no
        // `goal.md` is not a branch a checkpoint can fork off (§2.8).
        let dir = tempfile::TempDir::new().unwrap();
        assert!(reviewer_goal(dir.path(), "20260101-p1").is_err());
    }
}
