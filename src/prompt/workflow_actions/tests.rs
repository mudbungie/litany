//! Unit tests for the terminal-lifecycle seam and the ref-mark executor
//! (ARCH §6), over a recording git stub. The self-firing seam — a hook
//! forking a procedure child, the machinery base case, `user_message` —
//! is proved over a real workspace in `crate::prompt::tests::procedures`.

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
