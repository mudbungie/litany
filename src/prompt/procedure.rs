//! **Procedures** — the children a workflow forks of its own accord
//! (ARCH §6, `docs/DESIGN_PROCEDURES.md` §2), and the one home of what
//! makes such a child *machinery*.
//!
//! A procedure child is forked by a binding, not by a model's `dispatch`
//! call and not by an operator: nobody asked for it, and its whole
//! existence is an act of the workflow at a circumstance of the branch
//! itself. The circumstances a branch's own hop fires with no child
//! returning are the **self-firing events** ([`SELF_FIRING`]):
//! `user_message`, the three per-step hooks and `worker_flush`. A role a
//! `dispatch` under any of them names is machinery under that workflow
//! ([`machinery`]), and the set is *derived from the workflow* rather
//! than enumerated in code — so a `titler` bound at `user_message` is
//! machinery exactly as the compactor is, with nothing added here.
//!
//! **The invariant this module exists for: machinery is never the
//! subject of machinery** (bl-08b4, the return of bl-a9eb). A procedure
//! child inherits the dispatching branch's transcript and its workflow,
//! so without a base case its own `post_step` forks a titler, whose
//! `post_step` forks a titler; stated of the compactor alone it recurred
//! one role later for the reviewer (15 branches five levels deep from
//! one question). The set has one home and three readers, and they read
//! it as one derivation rather than as lists that could drift:
//!
//! - the compaction clock excludes a machinery branch at any count,
//!   elapsed time or elected flush ([`crate::prompt::compactor::checkpoint::due`]);
//! - a machinery branch **runs no self-firing binding** — no hook, no
//!   flush, no `user_message` ([`crate::prompt::workflow_actions::run_step_hook`]);
//! - a machinery child **keeps the inherited dialog** at its fork, since
//!   the dispatching branch's transcript is its subject
//!   ([`crate::prompt::dispatch::prune_inherited_dialog`]).
//!
//! A procedure child is instructed by the harness, not by a model, so its
//! goal is composed ([`goal`]): two roles own their builders, because
//! their prose names the procedure's injected tools and the compaction
//! point, which a soul cannot know; every other role gets the generic
//! goal, and what it *does* with that is its soul's (ARCH §4.3).

use super::{ChildDispatchRequest, Deps, Error, child_dispatch, compactor, reviewer, subagent};
use crate::config::version::Version;
use crate::config::{Action, Event, Workflow};
use std::collections::BTreeSet;
use std::path::Path;

/// The events a branch's own hop fires with no child returning and no
/// model asking (module docs; `docs/DESIGN_PROCEDURES.md` §2) — under
/// `version`: `user_message` is one of them exactly when that version
/// fires it ([`Version::fires_user_message`]). An event a version never
/// fires forks nothing, so its targets are not machinery under it — which
/// is what keeps every version-1 config's dead `user_message:
/// [dispatch(worker)]` from making every worker machinery.
pub(crate) fn self_firing(version: Version) -> Vec<Event> {
    let mut events = vec![
        Event::PreStep,
        Event::PostStep,
        Event::OnToolReturn,
        Event::WorkerFlush,
    ];
    if version.fires_user_message() {
        events.push(Event::UserMessage);
    }
    events
}

/// The actions bound to a self-firing `event`, or its baseline default
/// when unbound: `worker_flush` dispatches a compactor (ARCH §2.7, the
/// one self-firing event with a default), every other is the
/// empty-inputs no-op. The one reading both the executors and
/// [`machinery`] make, so the compactor stays machinery under a workflow
/// that never mentions it.
pub(crate) fn self_firing_actions(workflow: &Workflow, event: &Event) -> Vec<Action> {
    let bound = workflow.actions_for(event);
    if bound.is_empty() && *event == Event::WorkerFlush {
        return vec![Action::Dispatch {
            role: compactor::COMPACTOR_ROLE.to_string(),
            with: None,
            mode: None,
        }];
    }
    bound
}

/// The roles `workflow` at `version` forks of its own accord — every
/// `dispatch` target under a self-firing event, baseline defaults
/// included (module docs).
pub(crate) fn machinery(workflow: &Workflow, version: Version) -> BTreeSet<String> {
    self_firing(version)
        .into_iter()
        .flat_map(|event| self_firing_actions(workflow, &event))
        .filter_map(|action| match action {
            Action::Dispatch { role, .. } => Some(role),
            _ => None,
        })
        .collect()
}

/// Whether `role` is machinery under `workflow` at `version` ([`machinery`]).
pub(crate) fn is_machinery(workflow: &Workflow, version: Version, role: &str) -> bool {
    machinery(workflow, version).contains(role)
}

/// The goal a procedure child of `role` is dispatched with, read off the
/// **dispatching branch's** worktree so it can quote that branch's own
/// pinned `goal.md` (ARCH §2.7, §2.8). The compactor's and the reviewer's
/// are their own ([`compactor::compactor_goal`], [`reviewer::reviewer_goal`]);
/// every other role gets the generic form, and its soul says the rest.
pub(crate) fn goal(role: &str, worktree: &Path, agent_id: &str) -> Result<String, Error> {
    match role {
        compactor::COMPACTOR_ROLE => compactor::compactor_goal(worktree, agent_id),
        reviewer::REVIEWER_ROLE => reviewer::reviewer_goal(worktree, agent_id),
        _ => {
            let parent_goal = std::fs::read_to_string(worktree.join(subagent::GOAL_FILE))?;
            Ok(format!(
                "You are the {role} for branch `{agent_id}`.\n\nIts goal:\n{parent_goal}"
            ))
        }
    }
}

/// Fork one procedure child of `role` off `agent_id`'s branch at
/// `fork_point` — `None` is the tip — and start it through the front door
/// (ARCH §2.5, §2.7): the one door every self-firing dispatch spends.
/// What its return does is the returning role's business on a later hop.
pub(crate) fn dispatch(
    role: &str,
    workspace: &Path,
    agent_id: &str,
    worktree: &Path,
    fork_point: Option<&str>,
    deps: &Deps<'_>,
) -> Result<(), Error> {
    let goal = goal(role, worktree, agent_id)?;
    let req = ChildDispatchRequest {
        repo: workspace,
        parent_branch: agent_id,
        parent_worktree: worktree,
        role,
        goal: &goal,
        name: None,
        fork_point,
        cwd: None,
        pins: crate::prompt::PinnedDocs::none(),
    };
    child_dispatch::run_procedure(
        &req,
        deps.git,
        deps.clock,
        deps.id_gen,
        deps.launcher,
        deps.rng,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn wf(yaml: &str) -> Workflow {
        Workflow::parse(yaml, Path::new("workflow.yaml")).unwrap()
    }

    #[test]
    fn the_machinery_set_is_every_dispatch_under_a_self_firing_event() {
        // A titler at user_message and a guard at on_tool_return are
        // machinery; the verifier, dispatched at a child's return, is
        // not; a ref mark under a hook names no role at all.
        let w = wf(
            "events:\n  user_message: [dispatch(titler)]\n  on_tool_return: [dispatch(guard), notify_ui]\n  worker_return: [dispatch(verifier)]\n",
        );
        let set: Vec<String> = machinery(&w, Version(2)).into_iter().collect();
        assert_eq!(set, ["compactor", "guard", "titler"]);
        assert!(is_machinery(&w, Version(2), "titler"));
        assert!(!is_machinery(&w, Version(2), "verifier"));
        assert!(!is_machinery(&w, Version(2), "worker"));
    }

    #[test]
    fn a_version_that_never_fires_user_message_forks_nothing_there() {
        // Every version-1 config carries the template's dead
        // `user_message: [dispatch(worker)]`; under version 1 the event
        // never fires, so the worker it names is not machinery — or
        // every existing conversation's root would stop compacting.
        let w = wf("events:\n  user_message: [dispatch(worker)]\n");
        assert!(!is_machinery(&w, Version(1), "worker"));
        assert!(is_machinery(&w, Version(2), "worker"));
    }

    #[test]
    fn the_compactor_is_machinery_under_a_workflow_that_never_names_it() {
        // The `worker_flush` baseline (ARCH §2.7) is read here too, so
        // the exclusion holds for the basic loop's unbound shape.
        let w = wf("events: {}\n");
        assert!(is_machinery(&w, Version(1), compactor::COMPACTOR_ROLE));
        assert_eq!(machinery(&w, Version(2)).len(), 1);
        // A bound worker_flush replaces the baseline rather than adding
        // to it: the learning loop's two forks, and nothing implicit.
        let w = wf("events:\n  worker_flush: [dispatch(reviewer), dispatch(compactor)]\n");
        assert_eq!(machinery(&w, Version(2)).len(), 2);
        assert!(is_machinery(&w, Version(2), reviewer::REVIEWER_ROLE));
    }

    #[test]
    fn the_generic_goal_names_the_role_the_branch_and_its_goal() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("goal.md"), "ship the widget").unwrap();
        let g = goal("titler", dir.path(), "20260101-p1").unwrap();
        assert_eq!(
            g,
            "You are the titler for branch `20260101-p1`.\n\nIts goal:\nship the widget"
        );
        // The two role-owned builders answer for their own roles.
        assert!(
            goal(compactor::COMPACTOR_ROLE, dir.path(), "20260101-p1")
                .unwrap()
                .contains("compactor for branch")
        );
        assert!(
            goal(reviewer::REVIEWER_ROLE, dir.path(), "20260101-p1")
                .unwrap()
                .contains("reviewer for branch")
        );
    }

    #[test]
    fn a_goal_with_no_pinned_goal_file_is_an_io_error() {
        let dir = tempfile::tempdir().unwrap();
        let err = goal("titler", dir.path(), "20260101-p1").unwrap_err();
        assert!(matches!(err, Error::Io(_)), "{err:?}");
    }
}
