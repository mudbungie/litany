//! The workflow binding interpreter (ARCH §6 "The binding interpreter").
//!
//! `litany advance` *is* the interpreter (§6): one hop derives its
//! circumstance from disk and runs the bindings that match, holding no
//! cursor. This module is the terminal-lifecycle seam of that evaluation
//! — when a hop's step reached a terminal event, the epitaph names a
//! lifecycle event ([`lifecycle_event`]) and the workflow's bindings for
//! it are executed ([`run_terminal_bindings`]).
//!
//! The shipped executor covers the two git-native **ref-mark** actions,
//! the same pattern as the §2.6 `budget-exhausted` / `conflicted` marks
//! and read (never pushed) by the frontend (§3.5): `mark_abandoned`
//! writes `refs/litany/abandoned/<agent-id>` — a *non-derivable policy
//! assertion* ("this stopped branch will not be retried", distinct from
//! the derivable `stopped` classification, so it is a fact with a home,
//! not a mirror of one, `docs/PRINCIPLES.md` Single source of truth) —
//! and `notify_ui` writes `refs/litany/notify/<agent-id>`, the mark a
//! frontend surfaces as a user-facing notification. Every other action
//! in the closed set is declined loudly here ([`Error::ActionUnsupported`])
//! rather than silently dropped: its executor is a tracked follow-on of
//! this epic, and a binding that names it at a terminal must fail visibly,
//! never no-op (`docs/PRINCIPLES.md` "Decline illegal operations").

use crate::config::{Action, Event, Workflow};
use crate::prompt::Error;
use crate::prompt::inbox::Epitaph;
use crate::template::GitRunner;
use std::path::Path;

/// Ref prefix for the `mark_abandoned` action (ARCH §6): a git-native,
/// per-agent-id policy mark at the branch tip.
const ABANDONED_REF_PREFIX: &str = "refs/litany/abandoned/";
/// Ref prefix for the `notify_ui` action (ARCH §6, §3.5): a git-native,
/// per-agent-id user-facing-notification mark at the branch tip.
const NOTIFY_REF_PREFIX: &str = "refs/litany/notify/";

/// The lifecycle event a branch's *own* terminal names, by epitaph value
/// (ARCH §6 "Circumstance is derived from disk"; §2.6 — code branches on
/// the epitaph's value, never on message shape). Only `stopped` has a
/// distinct terminal-lifecycle binding today; `final-response` and
/// `budget-exhausted` terminate through the ordinary exit protocol and
/// name no event here, so they yield `None`.
pub(super) fn lifecycle_event(epitaph: Epitaph) -> Option<Event> {
    match epitaph {
        Epitaph::Stopped => Some(Event::BranchStopped),
        Epitaph::FinalResponse | Epitaph::BudgetExhausted | Epitaph::Died => None,
    }
}

/// Evaluate the workflow's bindings for the lifecycle event this branch's
/// terminal named (if any) and run each in declared order. A terminal
/// with no lifecycle event, or an event with no bindings, is a no-op —
/// the general path with empty inputs, not a special case. `worktree` is
/// the branch's materialized tree whose checked-out `HEAD` is the tip the
/// ref marks point at (§2.3).
pub(super) fn run_terminal_bindings(
    workflow: &Workflow,
    epitaph: Epitaph,
    worktree: &Path,
    agent_id: &str,
    git: &dyn GitRunner,
) -> Result<(), Error> {
    let Some(event) = lifecycle_event(epitaph) else {
        return Ok(());
    };
    run_event(workflow, event, worktree, agent_id, git)
}

/// Evaluate the workflow's bindings for a **per-step hook** event
/// (`pre_step` / `post_step` / `on_tool_return`, ARCH §6) and run each in
/// declared order. These fire on every branch, every step (the firing
/// points bracket the model call in [`crate::prompt::dispatch`]); the
/// shipped executor is the two git-native ref marks — the observability
/// surface (`notify_ui`) and the abandonment assertion (`mark_abandoned`).
/// Any other closed-set action in a hook (e.g. `dispatch`) is declined
/// loudly, its hook executor a tracked follow-on. An unbound hook is the
/// empty-inputs no-op.
pub(super) fn run_step_hook(
    workflow: &Workflow,
    event: Event,
    worktree: &Path,
    agent_id: &str,
    git: &dyn GitRunner,
) -> Result<(), Error> {
    run_event(workflow, event, worktree, agent_id, git)
}

/// Run every action bound to `event` in declared order — the shared
/// ref-mark executor behind both the terminal-lifecycle seam and the
/// per-step hooks. An unbound event runs nothing.
fn run_event(
    workflow: &Workflow,
    event: Event,
    worktree: &Path,
    agent_id: &str,
    git: &dyn GitRunner,
) -> Result<(), Error> {
    for action in workflow.actions_for(event) {
        execute(&action, event, worktree, agent_id, git)?;
    }
    Ok(())
}

/// Execute one action at a terminal-lifecycle `event`. The shipped subset
/// is the two ref marks; every remaining member of the closed set is
/// declined loudly, its executor a tracked follow-on of the §6
/// shipped-state note.
///
/// The match is **exhaustive by construction** — no `_` arm. That is the
/// totality guarantee for the vocabulary: a new [`Action`] variant cannot
/// be added without landing here and deciding, on the spot, whether it
/// executes or is an acknowledged deferral. Vocabulary that can reach
/// neither arm (`spawn_root_agent`, `spawn_exchange`) is not in the enum
/// at all — it is declined at parse (`config::action`).
pub(super) fn execute(
    action: &Action,
    event: Event,
    worktree: &Path,
    agent_id: &str,
    git: &dyn GitRunner,
) -> Result<(), Error> {
    match action {
        Action::MarkAbandoned => mark_ref(ABANDONED_REF_PREFIX, worktree, agent_id, git),
        Action::NotifyUi => mark_ref(NOTIFY_REF_PREFIX, worktree, agent_id, git),
        deferred @ (Action::Dispatch { .. }
        | Action::GateReturnOn { .. }
        | Action::DeliverResult
        | Action::LandCompaction
        | Action::StageProposal) => Err(Error::ActionUnsupported {
            action: format!("{deferred:?}"),
            event: event.as_str(),
        }),
    }
}

/// Write a git-native marker ref `<prefix><agent-id>` at the branch tip,
/// run inside `worktree` — whose checked-out branch *is* the agent's
/// branch (§2.3), so `HEAD` is the tip with no ref-name round trip. The
/// same shape as [`crate::prompt::budget::mark_exhausted`]; state lives in
/// git, never a sidecar (`docs/PRINCIPLES.md` Single source of truth).
fn mark_ref(
    prefix: &str,
    worktree: &Path,
    agent_id: &str,
    git: &dyn GitRunner,
) -> Result<(), Error> {
    let ref_name = format!("{prefix}{agent_id}");
    git.run(worktree, &["update-ref", ref_name.as_str(), "HEAD"])
        .map_err(|source| Error::Git {
            op: "workflow mark update-ref",
            source,
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;
    use std::io;

    /// A `GitRunner` recording every `run` argv, so ref-mark writes are
    /// asserted without a real git. `run_capture` is unused here.
    #[derive(Default)]
    struct RecGit {
        runs: RefCell<Vec<Vec<String>>>,
        fail: bool,
    }
    impl GitRunner for RecGit {
        fn run(&self, _dest: &Path, args: &[&str]) -> io::Result<()> {
            self.runs
                .borrow_mut()
                .push(args.iter().map(|s| s.to_string()).collect());
            if self.fail {
                Err(io::Error::other("update-ref failed"))
            } else {
                Ok(())
            }
        }
        fn run_capture(&self, _dest: &Path, _args: &[&str]) -> io::Result<String> {
            unreachable!("interpreter marks never capture")
        }
    }

    fn workflow(yaml: &str) -> Workflow {
        Workflow::parse(yaml, Path::new("workflow.yaml")).unwrap()
    }

    #[test]
    fn stopped_epitaph_names_branch_stopped_others_none() {
        assert_eq!(
            lifecycle_event(Epitaph::Stopped),
            Some(Event::BranchStopped)
        );
        assert_eq!(lifecycle_event(Epitaph::FinalResponse), None);
        assert_eq!(lifecycle_event(Epitaph::BudgetExhausted), None);
        assert_eq!(lifecycle_event(Epitaph::Died), None);
    }

    #[test]
    fn branch_stopped_runs_both_ref_marks_in_order() {
        let w = workflow("events:\n  branch_stopped:\n    - mark_abandoned\n    - notify_ui\n");
        let git = RecGit::default();
        run_terminal_bindings(&w, Epitaph::Stopped, Path::new("/wt"), "a-b", &git).unwrap();
        let runs = git.runs.borrow();
        assert_eq!(
            runs[0],
            vec!["update-ref", "refs/litany/abandoned/a-b", "HEAD"]
        );
        assert_eq!(
            runs[1],
            vec!["update-ref", "refs/litany/notify/a-b", "HEAD"]
        );
    }

    #[test]
    fn no_lifecycle_event_is_a_noop() {
        // A final-response terminal names no event: nothing runs even with
        // a fully-populated workflow.
        let w = workflow("events:\n  branch_stopped:\n    - mark_abandoned\n");
        let git = RecGit::default();
        run_terminal_bindings(&w, Epitaph::FinalResponse, Path::new("/wt"), "a-b", &git).unwrap();
        assert!(git.runs.borrow().is_empty());
    }

    #[test]
    fn unbound_event_is_a_noop() {
        // Stopped, but no branch_stopped binding: the empty-inputs path.
        let w = workflow("events: {}\n");
        let git = RecGit::default();
        run_terminal_bindings(&w, Epitaph::Stopped, Path::new("/wt"), "a-b", &git).unwrap();
        assert!(git.runs.borrow().is_empty());
    }

    #[test]
    fn unsupported_action_is_declined_loudly() {
        let w = workflow("events:\n  branch_stopped:\n    - dispatch(worker)\n");
        let git = RecGit::default();
        let err =
            run_terminal_bindings(&w, Epitaph::Stopped, Path::new("/wt"), "a-b", &git).unwrap_err();
        match err {
            Error::ActionUnsupported { action, event } => {
                assert!(action.contains("Dispatch"), "got {action}");
                assert_eq!(event, "branch_stopped");
            }
            other => panic!("expected ActionUnsupported, got {other:?}"),
        }
    }

    #[test]
    fn ref_mark_git_failure_is_surfaced() {
        let w = workflow("events:\n  branch_stopped:\n    - mark_abandoned\n");
        let git = RecGit {
            fail: true,
            ..Default::default()
        };
        let err =
            run_terminal_bindings(&w, Epitaph::Stopped, Path::new("/wt"), "a-b", &git).unwrap_err();
        assert!(matches!(err, Error::Git { .. }), "got {err:?}");
    }
}
