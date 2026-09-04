//! Integration tests for the workflow action DSL parser.

use crate::config::action::{Action, DispatchMode};

#[test]
fn parses_zero_arg_actions() {
    assert_eq!(
        Action::parse("land_compaction").unwrap(),
        Action::LandCompaction
    );
    // The retired spelling (the merge-back landing, replaced by
    // rebase-forward, bl-bc9c) still parses: configs are frozen commits
    // and a running workspace's vocabulary must keep resolving.
    assert_eq!(
        Action::parse("compaction_merge").unwrap(),
        Action::LandCompaction
    );
    assert_eq!(
        Action::parse("deliver_result").unwrap(),
        Action::DeliverResult
    );
    assert_eq!(
        Action::parse("mark_abandoned").unwrap(),
        Action::MarkAbandoned
    );
    assert_eq!(Action::parse("notify_ui").unwrap(), Action::NotifyUi);
    // The reviewer's landing parses today and is declined by the
    // interpreter until it ships (`docs/DESIGN_LEARNING_LOOP.md` §3).
    assert_eq!(
        Action::parse("stage_proposal").unwrap(),
        Action::StageProposal
    );
    assert!(Action::parse("stage_proposal(skills)").is_err());
}

#[test]
fn parses_dispatch_role_only() {
    assert_eq!(
        Action::parse("dispatch(worker)").unwrap(),
        Action::Dispatch {
            role: "worker".into(),
            with: None,
            mode: None
        }
    );
}

#[test]
fn parses_dispatch_with_kwarg() {
    assert_eq!(
        Action::parse("dispatch(worker, with: verifier.feedback)").unwrap(),
        Action::Dispatch {
            role: "worker".into(),
            with: Some("verifier.feedback".into()),
            mode: None
        }
    );
}

#[test]
fn parses_dispatch_with_mode() {
    assert_eq!(
        Action::parse("dispatch(compactor, mode: intermediate)").unwrap(),
        Action::Dispatch {
            role: "compactor".into(),
            with: None,
            mode: Some(DispatchMode::Intermediate)
        }
    );
}

#[test]
fn parses_gate_return_on() {
    assert_eq!(
        Action::parse("gate_return_on(verifier.approve)").unwrap(),
        Action::GateReturnOn {
            predicate: "verifier.approve".into()
        }
    );
}

#[test]
fn rejects_unknown_action() {
    let e = Action::parse("teleport(worker)").unwrap_err();
    assert!(e.contains("unknown action"), "got: {e}");
}

/// `spawn_root_agent` and `spawn_exchange` were subtracted from the
/// vocabulary (bl-0e79): ARCH §2.4 leaves no circumstance a hop could fire
/// them from — a user message resumes the agent's own branch, a new root
/// agent is forked explicitly, and an exchange "owns no branch, no merge,
/// no lifecycle". A config still naming one is declined with the reason,
/// never accepted and silently ignored — the same idiom as the retired
/// `overflow: summarize` (bl-a1a1).
#[test]
fn declines_retired_spawn_vocabulary_with_a_reason() {
    let e = Action::parse("spawn_root_agent").unwrap_err();
    assert!(e.contains("was retired"), "got: {e}");
    assert!(e.contains("ARCH §2.4"), "got: {e}");
    assert!(e.contains("remove the binding"), "got: {e}");

    let e = Action::parse("spawn_exchange").unwrap_err();
    assert!(e.contains("was retired"), "got: {e}");
    assert!(e.contains("UX span"), "got: {e}");

    // Retirement is by name, not by shape: the arity check never runs.
    let e = Action::parse("spawn_exchange(now)").unwrap_err();
    assert!(e.contains("was retired"), "got: {e}");
}

#[test]
fn rejects_zero_arg_action_with_args() {
    let e = Action::parse("compaction_merge(now)").unwrap_err();
    assert!(e.contains("no arguments"), "got: {e}");
}

#[test]
fn rejects_dispatch_without_role() {
    assert!(Action::parse("dispatch()").is_err());
    assert!(Action::parse("dispatch(with: x)").is_err());
}

#[test]
fn rejects_dispatch_unknown_kwarg() {
    let e = Action::parse("dispatch(worker, retries: 3)").unwrap_err();
    assert!(e.contains("unknown named arg"), "got: {e}");
}

#[test]
fn rejects_dispatch_unknown_mode() {
    let e = Action::parse("dispatch(worker, mode: terminal)").unwrap_err();
    assert!(e.contains("unknown mode"), "got: {e}");
}

#[test]
fn rejects_dispatch_extra_positional() {
    let e = Action::parse("dispatch(worker, extra)").unwrap_err();
    assert!(e.contains("at most one positional"), "got: {e}");
}

#[test]
fn rejects_gate_return_on_arity() {
    assert!(Action::parse("gate_return_on()").is_err());
    assert!(Action::parse("gate_return_on(a, b)").is_err());
}

#[test]
fn rejects_unbalanced_parens() {
    assert!(Action::parse("dispatch(worker").is_err());
}

#[test]
fn rejects_invalid_identifier() {
    assert!(Action::parse("bad-action").is_err());
}

#[test]
fn rejects_empty_argument() {
    assert!(Action::parse("dispatch(worker, , mode: intermediate)").is_err());
}

#[test]
fn rejects_invalid_value() {
    assert!(Action::parse("dispatch(worker!)").is_err());
    assert!(Action::parse("dispatch(worker, with: bad value)").is_err());
}

#[test]
fn rejects_invalid_kwarg_key() {
    assert!(Action::parse("dispatch(worker, bad-key: x)").is_err());
}

#[test]
fn rejects_empty_action_name() {
    let e = Action::parse("(worker)").unwrap_err();
    assert!(e.contains("empty identifier"), "got: {e}");
}

#[test]
fn rejects_empty_kwarg_value() {
    let e = Action::parse("dispatch(worker, mode: )").unwrap_err();
    assert!(e.contains("empty value"), "got: {e}");
}
