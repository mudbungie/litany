//! Integration tests for `workflow.yaml` parsing and validation.

use crate::config::action::{Action, DispatchMode};
use crate::config::error::LoadError;
use crate::config::workflow::{Backoff, Budgets, CompactionTrigger, Event, RetryConfig, Workflow};
use std::path::Path;
use std::time::Duration;

/// Parse workflow YAML the way the runtime does — from content in hand,
/// labelled with the `<commit>:<path>` origin the config commit gives it
/// (ARCH §2.2). There is no file-loading variant to test.
fn parse(raw: &str) -> Result<Workflow, LoadError> {
    Workflow::parse(raw, Path::new("<commit>:workflow.yaml"))
}

// YAML interprets `key: value` inside a plain scalar as a map entry,
// so any action with named arguments must be quoted. Unambiguous actions
// (no `: ` inside) may stay bare.
const ARCH_EXAMPLE: &str = r#"
events:
  user_message:
    - dispatch(worker)
  worker_return:
    - dispatch(verifier)
    - gate_return_on(verifier.approve)
  verifier_approve:
    - deliver_result
  verifier_reject:
    - "dispatch(worker, with: verifier.feedback)"
  worker_flush:
    - "dispatch(compactor, mode: intermediate)"
  compactor_return:
    - land_compaction
  branch_stopped:
    - mark_abandoned
    - notify_ui
  pre_step: []
  post_step: []
  on_tool_return: []

compaction:
  intermediate:
    trigger: every_n_commits
    n: 10
"#;

#[test]
fn parses_arch_example() {
    let w = parse(ARCH_EXAMPLE).unwrap();
    let typed = w.typed_events();
    assert_eq!(typed[&Event::UserMessage].len(), 1);
    assert_eq!(
        typed[&Event::WorkerFlush][0],
        Action::Dispatch {
            role: "compactor".into(),
            with: None,
            mode: Some(DispatchMode::Intermediate)
        }
    );
    assert_eq!(
        w.compaction.as_ref().unwrap().intermediate.trigger,
        CompactionTrigger::EveryNCommits
    );
}

#[test]
fn parses_explicit_retry_block() {
    // Covers the RetryConfig + Backoff deserialize path (ARCH §6, §2.10).
    let w = parse("events:\n  user_message:\n    - land_compaction\nretry:\n  max_attempts: 5\n  backoff: exponential\n").unwrap();
    assert_eq!(w.retry.max_attempts, 5);
    assert_eq!(w.retry.backoff, Backoff::Exponential);
    // Exponential backoff doubles from the first rung.
    let d1 = w.retry.backoff.delay(1, None);
    let d2 = w.retry.backoff.delay(2, None);
    assert!(d2 > d1 && d1 > Duration::ZERO);
}

#[test]
fn absent_pacing_hint_leaves_the_config_schedule_alone() {
    // ARCH §4.4: no `retry_after_seconds` → behavior is the pure
    // config-driven schedule, keyed off the attempt number.
    let b = Backoff::Exponential;
    assert_eq!(b.delay(1, None), Duration::from_millis(250));
    assert_eq!(b.delay(3, None), Duration::from_millis(1000));
}

#[test]
fn pacing_hint_below_the_schedule_does_not_shrink_it() {
    // The hint is a floor, never a shrink: 0s under a 250ms rung is
    // indistinguishable from no hint at all.
    let b = Backoff::Exponential;
    assert_eq!(b.delay(1, Some(0)), b.delay(1, None));
    // Rung 12 is 250ms << 2^11 = 512s, well past a 1s hint.
    assert_eq!(b.delay(12, Some(1)), b.delay(12, None));
    assert!(b.delay(12, Some(1)) > Duration::from_secs(1));
}

#[test]
fn pacing_hint_above_the_schedule_wins() {
    // A provider asking for 30s outranks the 250ms first rung.
    let b = Backoff::Exponential;
    assert_eq!(b.delay(1, Some(30)), Duration::from_secs(30));
    assert_eq!(b.delay(2, Some(30)), Duration::from_secs(30));
}

#[test]
fn omitted_retry_block_uses_the_default() {
    // No `retry:` → RetryConfig::default (3 attempts, exponential).
    let w = parse("events:\n  user_message:\n    - land_compaction\n").unwrap();
    assert_eq!(w.retry, RetryConfig::default());
    assert_eq!(w.retry.max_attempts, 3);
    assert_eq!(w.retry.backoff, Backoff::Exponential);
}

#[test]
fn parses_explicit_budgets_block() {
    // ARCH §6 budgets example: all three limits declared.
    let w = parse("events: {}\nbudgets:\n  max_total_tokens: 2000000\n  max_wall_seconds: 3600\n  max_depth: 4\n").unwrap();
    assert_eq!(w.budgets.max_total_tokens, Some(2_000_000));
    assert_eq!(w.budgets.max_wall_seconds, Some(3600));
    assert_eq!(w.budgets.max_depth, Some(4));
}

#[test]
fn omitted_budgets_block_is_all_unbounded() {
    // No `budgets:` → Budgets::default (every axis None = unbounded).
    let w = parse("events:\n  user_message:\n    - land_compaction\n").unwrap();
    assert_eq!(w.budgets, Budgets::default());
    assert!(w.budgets.max_total_tokens.is_none());
    assert!(w.budgets.max_wall_seconds.is_none());
    assert!(w.budgets.max_depth.is_none());
}

#[test]
fn the_shipped_template_declares_no_budgets_and_is_unbounded() {
    // Operator ruling 2026-08-16 (ARCH §6 "Nothing ships bounded"): the
    // shipped `workflow.yaml` declares no `budgets:` block, so a
    // template-born workspace is unbounded on every axis — including
    // `max_depth`. These are the exact bytes `litany new` writes into
    // the first config commit (pinned by template/tests_override.rs),
    // so this is the workspace's own state, not just the parser's.
    let raw = crate::template::TEMPLATE
        .get_file("workflow.yaml")
        .expect("the template ships a workflow.yaml")
        .contents_utf8()
        .expect("utf8");
    assert!(
        !raw.contains("\nbudgets:"),
        "the shipped template must declare no budgets block"
    );
    let w = parse(raw).unwrap();
    assert_eq!(w.budgets, Budgets::default());
    assert!(w.budgets.max_total_tokens.is_none());
    assert!(w.budgets.max_wall_seconds.is_none());
    assert!(w.budgets.max_depth.is_none());
}

#[test]
fn partial_budgets_leaves_the_other_axes_unbounded() {
    // A single declared limit; the rest stay unbounded (§6).
    let w = parse("events: {}\nbudgets:\n  max_total_tokens: 500\n").unwrap();
    assert_eq!(w.budgets.max_total_tokens, Some(500));
    assert!(w.budgets.max_wall_seconds.is_none());
    assert!(w.budgets.max_depth.is_none());
}

#[test]
fn parses_explicit_tool_output_block() {
    // ARCH §3.3 bounded transcript projection: per-stream head+tail
    // byte allowances, read from `workflow.yaml` (§6).
    let w = parse("events: {}\ntool_output:\n  head_bytes: 16384\n  tail_bytes: 16384\n").unwrap();
    let bound = w.tool_output.unwrap();
    assert_eq!(bound.head_bytes, 16384);
    assert_eq!(bound.tail_bytes, 16384);
}

#[test]
fn omitted_tool_output_block_is_unbounded() {
    // No `tool_output:` → the projection is unbounded — the policy is
    // severable (the shipped default lives in template/workflow.yaml).
    let w = parse("events: {}\n").unwrap();
    assert!(w.tool_output.is_none());
}

#[test]
fn rejects_unknown_tool_output_fields() {
    // deny_unknown_fields: a misspelled knob is a parse error, not a
    // silently-unbounded stream.
    let err =
        parse("events: {}\ntool_output:\n  head_bytes: 1\n  tail_bytes: 2\n  middle_bytes: 3\n")
            .unwrap_err();
    assert!(matches!(err, LoadError::Yaml { .. }));
}

#[test]
fn parses_the_tool_control_block() {
    // ARCH §3.3 *Tool control*: the seam's one config knob — the
    // adjudicator binary the tool window consults (§6).
    let w = parse("events: {}\ntool_control:\n  command: /opt/controls/guardian\n").unwrap();
    assert_eq!(w.tool_control.unwrap().command, "/opt/controls/guardian");
}

#[test]
fn omitted_tool_control_consults_nothing() {
    // No `tool_control:` → no control in the path — the seam is
    // severable, and no control ships (template/workflow.yaml omits it).
    let w = parse("events: {}\n").unwrap();
    assert!(w.tool_control.is_none());
}

#[test]
fn rejects_an_empty_tool_control_command() {
    let err = parse("events: {}\ntool_control:\n  command: \"  \"\n").unwrap_err();
    match err {
        LoadError::Invalid { key, message, .. } => {
            assert_eq!(key, "tool_control.command");
            assert!(message.contains("control executable"), "{message}");
        }
        other => panic!("expected Invalid, got {other:?}"),
    }
}

#[test]
fn rejects_unknown_tool_control_fields() {
    // deny_unknown_fields: policy the seam does not understand (per-tool
    // filters, args) belongs in the control, and a knob it would ignore
    // is declined, never accepted.
    let err = parse("events: {}\ntool_control:\n  command: c\n  tools: [bash]\n").unwrap_err();
    assert!(matches!(err, LoadError::Yaml { .. }));
}

#[test]
fn rejects_unknown_event() {
    let err = parse("events:\n  user_request:\n    - notify_ui\n").unwrap_err();
    assert!(matches!(err, LoadError::Yaml { .. }));
}

#[test]
fn rejects_unknown_action() {
    let err = parse("events:\n  user_message:\n    - teleport(worker)\n").unwrap_err();
    match err {
        LoadError::Invalid { key, message, .. } => {
            assert_eq!(key, "events.user_message[0]");
            assert!(message.contains("unknown action"));
        }
        other => panic!("expected Invalid, got {other:?}"),
    }
}

const EVENT_NAMES: &[&str] = &[
    "user_message",
    "worker_return",
    "verifier_approve",
    "verifier_reject",
    "worker_flush",
    "compactor_return",
    "branch_stopped",
    "pre_step",
    "post_step",
    "on_tool_return",
];

#[test]
fn each_event_name_round_trips() {
    for name in EVENT_NAMES {
        let yaml = format!("events:\n  {name}: []\n");
        parse(&yaml).unwrap_or_else(|e| panic!("event {name} did not round-trip: {e}"));
    }
}

// Exercises the per-event branches of the internal `event_name` map by
// triggering an invalid-action error under every event.
#[test]
fn invalid_action_error_message_names_each_event() {
    for name in EVENT_NAMES {
        let yaml = format!("events:\n  {name}:\n    - bogus_action\n");
        let err = parse(&yaml).unwrap_err();
        match err {
            LoadError::Invalid { key, .. } => {
                assert!(
                    key.contains(name),
                    "expected key to contain {name}, got {key}"
                );
            }
            other => panic!("expected Invalid for {name}, got {other:?}"),
        }
    }
}
