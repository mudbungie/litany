//! Compaction (ARCH §2.6, §2.7) — the model-driven compactor, its
//! checkpoint triggers, and the compaction landing (rebase-forward).
//!
//! A **compactor** is an *ordinary child agent* (§2.7): it is dispatched
//! like any other child ([`super::child_dispatch`] with the `compactor`
//! role), runs its own step loop with a real model call, and returns by
//! depositing a result message (§2.6). Nothing about it is privileged
//! except two things this module owns:
//!
//! - its **toolset** ([`tools`]) — the fixed pair `write_summary` /
//!   `mark_for_deletion`, built into the primitive and available to the
//!   compactor role alone, making "deletion-only" structural (§2.7);
//! - how its output **lands** ([`land`]) — the compaction landing, a
//!   **rebase-forward**: the span before the compaction point squashes
//!   into a compaction base and the live tail replays on top (§2.6).
//!   Nothing merges anywhere anymore; the merge-back landing is retired
//!   (bl-bc9c).
//!
//! [`checkpoint`] is the trigger evaluation: the executor reads it at a
//! step boundary and, when a checkpoint is due, dispatches a compactor off
//! the **compaction point** — the branch tip, or `HEAD~keep_recent` when
//! the workflow retains a recent tail (§2.6, §6). When the compactor
//! returns with a *final-response* epitaph, the executor lands the
//! rebase-forward; any other epitaph lands **nothing** and the branch
//! continues uncompacted (§2.7). Both the boundary trigger read and the
//! return-time landing are the same step-boundary seam the
//! workflow-binding interpreter drives (§6) — [`checkpoint::due`] and
//! [`land::land`] are the binding-shaped procedures it invokes.
//!
//! **There is no terminal-compaction stage** (§2.7): a child's result
//! message carries its own terminal response (§2.6), not a compactor
//! product, and with merge-back gone there is no merge payload to slim
//! before returning. The v0.3 terminal compactor — a stub dispatched at
//! every final response — is deleted; compaction now happens only at
//! configured checkpoints during a branch's life.

pub mod checkpoint;
pub mod land;
pub mod tools;

pub use checkpoint::{due, state};
pub use land::{LandOutcome, land};

use super::tool::inject::InjectedTool;
use super::{Error, subagent};
use serde_json::json;
use std::path::Path;

/// Role name of the compactor child (ARCH §2.7). Its soul is
/// `souls/compactor.md` in the governing config commit, and its toolset is
/// the built-in [`tools`] pair — not a `providers.yaml` `tools:` list.
pub const COMPACTOR_ROLE: &str = "compactor";

/// What `role`'s **procedure** injects into its next model request — the
/// compactor's fixed pair for the compactor role, nothing for any other
/// (ARCH §2.7, §6 role-aware resolution).
///
/// These are built into the primitive, never declared in
/// `providers.yaml` and never sourced from `descriptions/**` (a
/// compactor's inherited tree carries the dispatching branch's worker
/// schemas, not these), so the harness supplies the definitions directly
/// — the one place the model is told it may call `write_summary` /
/// `mark_for_deletion`. Narrow by construction: two tools, deletion-only,
/// making "the worst case is lost, never corrupted, information" a
/// structural property (§2.7).
///
/// **One fact, two readings.** This is also what makes a role's
/// *effective toolset* — its `providers.yaml` `tools:` grant plus
/// whatever is injected — computable without a second role table: the
/// composer reaches for the definitions
/// ([`super::dispatch::tools::compose`]) and the execution gate for their
/// names ([`super::dispatch::tool_step`]), off this one list. It was two
/// functions until bl-9001, a schema half and a names half held in step
/// by a test; they are now one list read two ways, which is the same
/// discipline the host injection ([`super::tool::inject`]) rides.
///
/// The compactor's `tools:` grant is empty in every shipped config
/// (`template/providers.yaml`, held by
/// `src/install/tests.rs::the_shipped_worker_grant_is_the_whole_tool_pool`),
/// so its effective toolset *is* this pair — the deletion-only guarantee
/// of §2.7 stated as the general rule rather than a role-shaped branch in
/// the executor.
pub fn builtin_tool_schemas(role: &str) -> Vec<InjectedTool> {
    if role != COMPACTOR_ROLE {
        return Vec::new();
    }
    vec![
        InjectedTool {
            name: tools::WRITE_SUMMARY.to_string(),
            description: Some(
                "Write a signal-preserving summary to the next summary/<NNN>.md \
                 on this branch."
                    .to_string(),
            ),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "content": {
                        "type": "string",
                        "description": "The summary body, written verbatim."
                    }
                },
                "required": ["content"],
                "additionalProperties": false
            }),
        },
        InjectedTool {
            name: tools::MARK_FOR_DELETION.to_string(),
            description: Some(
                "Nominate a branch-relative path for removal (deletion-only: this \
                 can remove, never rewrite)."
                    .to_string(),
            ),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "path": {
                        "type": "string",
                        "description": "Branch-relative path to remove, e.g. messages/003-user.md."
                    }
                },
                "required": ["path"],
                "additionalProperties": false
            }),
        },
    ]
}

/// Boilerplate goal handed to a compactor at dispatch (ARCH §2.7), read
/// off the **dispatching branch's worktree** so the compactor's own goal
/// can quote that branch's goal verbatim.
///
/// Every source it names is one the compactor actually receives (§2.7,
/// bl-2c63). Its inherited worktree — forked off the checkpoint commit —
/// carries the dispatching branch's whole tree, but only what the
/// `compactor` manifest entry selects composes (§5.1: the tree bounds,
/// the manifest selects), and that is the unconditional transcript tail
/// plus `order: [summary/**]` (§5.2). Work products are deliberately
/// neither composed nor named here: the acts that produced them are
/// already transcript entries, and the compaction product is a view of
/// the branch's *history* (§2.6 filter). The dispatching branch's goal
/// is the third source, and it has no other route in — the child's own
/// `goal.md` is this text — so it is quoted inline, fixed at dispatch
/// from a file that is never rewritten (§2.8), which is why the quote
/// cannot drift from its source.
pub fn compactor_goal(parent_worktree: &Path, parent_branch: &str) -> Result<String, Error> {
    let parent_goal = std::fs::read_to_string(parent_worktree.join(subagent::GOAL_FILE))?;
    Ok(format!(
        "You are the compactor for branch `{parent_branch}`.\n\
         \n\
         In your context is that branch's transcript and its prior summaries\n\
         under `summary/`. Read them and produce a signal-preserving, minimal\n\
         view of the branch's history using the `write_summary` tool. The\n\
         harness writes it to the next `summary/<NNN>.md` on this branch. Carry\n\
         a prior summary's signal forward into what you write before you\n\
         nominate it for deletion: once deleted, its content is gone from the\n\
         branch's context for good.\n\
         \n\
         Use `mark_for_deletion` to nominate superseded files — stale transcript\n\
         entries under `messages/`, a prior `summary/` you are replacing, spent\n\
         `skills/` bodies the transcript shows loaded and finished with. The\n\
         branch's dispatch entry — `messages/001-…`, its opening prompt — is the\n\
         goal quoted below in transcript form, is never superseded, and a\n\
         nomination of it is declined. So is a nomination of the summary you\n\
         wrote this pass: it is the only thing your compaction leaves behind.\n\
         An earlier pass's summary is a different thing and is yours to\n\
         supersede. Your\n\
         toolset is deletion-only: you can remove and summarize, never rewrite,\n\
         so the worst case is lost information, never corrupted\n\
         information. A work product the live branch has rewritten\n\
         since you forked is kept regardless of your nomination (live-branch-wins).\n\
         \n\
         Judge relevance against the dispatching branch's own goal, not your own\n\
         preferences:\n\
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
    fn compactor_goal_names_the_branch_and_both_tools() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("goal.md"), "ship the widget").unwrap();
        let g = compactor_goal(dir.path(), "20260101-p1").unwrap();
        assert!(g.contains("`20260101-p1`"), "{g}");
        assert!(g.contains("write_summary"));
        assert!(g.contains("mark_for_deletion"));
        assert!(g.contains("summary/<NNN>.md"));
        assert!(g.contains("live-branch-wins"));
    }

    #[test]
    fn compactor_goal_quotes_the_dispatching_branchs_goal_and_names_only_reachable_sources() {
        // bl-2c63: every source the boilerplate names is one the compactor
        // receives — the transcript tail, `summary/**` (its manifest entry's
        // one `order` category, §5.2), and the dispatching branch's goal,
        // which reaches it only by this quote (its own `goal.md` is this
        // text). Work products are named nowhere: nothing composes them and
        // the role has no read tool (§2.7).
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("goal.md"), "ship the widget").unwrap();
        let g = compactor_goal(dir.path(), "20260101-p1").unwrap();
        assert!(
            g.contains("<dispatching-branch-goal>\nship the widget\n</dispatching-branch-goal>"),
            "{g}"
        );
        assert!(g.contains("summary/"));
        assert!(g.contains("messages/"));
        assert!(!g.contains("work products"), "{g}");
    }

    #[test]
    fn compactor_goal_declines_a_dispatching_branch_with_no_goal() {
        let dir = tempfile::tempdir().unwrap();
        let err = compactor_goal(dir.path(), "20260101-p1").unwrap_err();
        assert!(matches!(err, Error::Io(_)), "{err:?}");
    }

    #[test]
    fn compactor_goal_states_that_the_dispatch_entry_is_never_nominable() {
        // bl-898f: the harness declines a nomination of the dispatch entry
        // (§2.7 — the goal is not compaction-eligible), so the boilerplate
        // says so. A refusal the model was never told about arrives as a
        // surprise `is_error`; stated, it is a rule it can work within.
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("goal.md"), "ship the widget").unwrap();
        let g = compactor_goal(dir.path(), "20260101-p1").unwrap();
        assert!(g.contains("dispatch entry"), "{g}");
        assert!(g.contains("messages/001-"), "{g}");
        assert!(g.contains("declined"), "{g}");
    }

    #[test]
    fn compactor_goal_states_that_this_passs_own_summary_is_not_nominable() {
        // bl-c7bb, on bl-898f's reasoning: a refusal the model was never
        // told about arrives as a surprise `is_error`, and this one fires
        // on the tool call the *other* half of the pair invited. Stated
        // with its counterpart — an earlier pass's summary stays the
        // thing it is told to supersede — so the rule reads as a
        // distinction rather than a ban on the directory.
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("goal.md"), "ship the widget").unwrap();
        let g = compactor_goal(dir.path(), "20260101-p1").unwrap();
        assert!(g.contains("the summary you\nwrote this pass"), "{g}");
        assert!(g.contains("An earlier pass's summary"), "{g}");
    }

    #[test]
    fn compactor_role_is_the_soul_key() {
        assert_eq!(COMPACTOR_ROLE, "compactor");
    }

    #[test]
    fn builtin_tool_schemas_are_the_two_named_tools_with_required_inputs() {
        let injected = builtin_tool_schemas(COMPACTOR_ROLE);
        let names: Vec<&str> = injected.iter().map(|t| t.name.as_str()).collect();
        assert_eq!(names, vec![tools::WRITE_SUMMARY, tools::MARK_FOR_DELETION]);
        // Each carries a description and a required input field, so the
        // model is told the tool's shape (§2.7 injected toolset).
        assert!(injected.iter().all(|t| t.description.is_some()));
        assert_eq!(injected[0].input_schema["required"][0], "content");
        assert_eq!(injected[1].input_schema["required"][0], "path");
    }

    #[test]
    fn no_other_role_has_an_injected_toolset() {
        assert!(builtin_tool_schemas("worker").is_empty());
        assert!(builtin_tool_schemas("verifier").is_empty());
    }
}
