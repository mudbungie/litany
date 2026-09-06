//! **Checkpoint children** — the roles the harness itself mints at a
//! checkpoint, and the one place that set is written down (ARCH §2.7).
//!
//! A checkpoint child is *machinery*: nobody asked for it, no model
//! dispatched it, and its whole existence is an act of the `worker_flush`
//! binding at a step boundary (§6). Two roles are of that kind today —
//! the **compactor**, whose product lands by rebase-forward (§2.6), and
//! the **reviewer** forked beside it off the same compaction point
//! (`docs/DESIGN_LEARNING_LOOP.md` §2).
//!
//! **The invariant this module exists for: machinery is never the
//! subject of machinery.** A branch the harness minted at a checkpoint is
//! not compaction-eligible, at any commit count, elapsed time or elected
//! flush ([`crate::prompt::compactor::checkpoint::due`]). Stated of the
//! *compactor* alone it recurred one role later: a reviewer forks with
//! the inherited transcript exactly as a compactor does, crosses its own
//! `every_n_commits` boundary, fires its own `worker_flush`, and mints a
//! compactor **and** a reviewer — each of which does the same, with no
//! depth term and no base case (bl-08b4, the return of bl-a9eb).
//!
//! So the set has **one home and two readers**, and they read it as
//! function values rather than as two lists that could drift:
//! [`checkpoint_goal`] answers *which roles the harness can instruct at a
//! checkpoint*, the flush dispatches exactly those (any other role is a
//! config fault it declines loudly, `docs/PRINCIPLES.md` "Decline illegal
//! operations"), and [`is_checkpoint_child`] — the same `match`, asked
//! for presence rather than for the goal — excludes exactly those from
//! the compaction-eligible set. A role added to the one is excluded from
//! the other in the same edit, which is what keeps a future role from
//! recurring this a third time.

use super::{Error, compactor, reviewer};
use std::path::Path;

/// The boilerplate goal a checkpoint child is dispatched with, read off
/// the **dispatching branch's** worktree (§2.7): both arms quote that
/// branch's own pinned `goal.md`, so both take the dispatching worktree
/// and the dispatching agent's id.
pub(crate) type CheckpointGoal = fn(&Path, &str) -> Result<String, Error>;

/// The goal builder for `role` when the harness mints that role at a
/// checkpoint, or `None` when it does not. **The one home of the
/// checkpoint-role set** (module docs): `None` is simultaneously "the
/// flush cannot instruct this fork" and "this role is an ordinary
/// compaction subject".
pub(crate) fn checkpoint_goal(role: &str) -> Option<CheckpointGoal> {
    match role {
        compactor::COMPACTOR_ROLE => Some(compactor::compactor_goal),
        reviewer::REVIEWER_ROLE => Some(reviewer::reviewer_goal),
        _ => None,
    }
}

/// Whether `role` names a **checkpoint child** — machinery the harness
/// minted, never work an agent asked for. Read by the eligibility
/// predicate (§2.7): machinery is never the subject of machinery.
pub(crate) fn is_checkpoint_child(role: &str) -> bool {
    checkpoint_goal(role).is_some()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_harness_minted_roles_are_exactly_the_ones_it_can_instruct() {
        // One `match`, both readings: a role the flush can instruct is a
        // role the eligible set excludes, and the compiler cannot let the
        // two disagree because there is only one of them.
        for role in [compactor::COMPACTOR_ROLE, reviewer::REVIEWER_ROLE] {
            assert!(checkpoint_goal(role).is_some(), "{role} is instructable");
            assert!(is_checkpoint_child(role), "{role} is machinery");
        }
    }

    #[test]
    fn an_ordinary_role_is_neither_instructable_nor_machinery() {
        for role in ["worker", "verifier", ""] {
            assert!(checkpoint_goal(role).is_none(), "{role:?}");
            assert!(!is_checkpoint_child(role), "{role:?}");
        }
    }

    #[test]
    fn the_goal_builder_it_hands_back_is_the_roles_own() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("goal.md"), "ship the widget").unwrap();
        let compactor_text =
            checkpoint_goal(compactor::COMPACTOR_ROLE).unwrap()(dir.path(), "20260101-p1").unwrap();
        assert!(
            compactor_text.contains("compactor for branch"),
            "{compactor_text}"
        );
        let reviewer_text =
            checkpoint_goal(reviewer::REVIEWER_ROLE).unwrap()(dir.path(), "20260101-p1").unwrap();
        assert!(
            reviewer_text.contains("reviewer for branch"),
            "{reviewer_text}"
        );
    }
}
