//! `workflow.yaml` `compaction:` block validation (ARCH §2.6, §2.7,
//! §6) — split from [`workflow_yaml`](super::workflow_yaml) to hold the
//! per-file line cap.

use crate::config::error::LoadError;
use crate::config::workflow::Workflow;
use std::path::Path;

/// Same origin-labelled parse as `workflow_yaml`'s.
fn parse(raw: &str) -> Result<Workflow, LoadError> {
    Workflow::parse(raw, Path::new("<commit>:workflow.yaml"))
}

#[test]
fn workflow_without_compaction_is_ok() {
    let w = parse("events:\n  user_message:\n    - notify_ui\n").unwrap();
    assert!(w.compaction.is_none());
}

#[test]
fn rejects_a_retained_tail_at_or_over_the_commit_trigger() {
    // §2.6/§6: keep_recent >= n under every_n_commits would leave the
    // clock over threshold at every landing — refused at load. Below n
    // parses; the time trigger is unconstrained (the clock is seconds).
    let yaml = "events: {}\ncompaction:\n  intermediate:\n    trigger: every_n_commits\n    n: 5\n    keep_recent: 5\n";
    match parse(yaml).unwrap_err() {
        LoadError::Invalid { key, message, .. } => {
            assert_eq!(key, "compaction.intermediate.keep_recent");
            assert!(message.contains("smaller than n"), "{message}");
        }
        other => panic!("expected Invalid, got {other:?}"),
    }
    let ok = "events: {}\ncompaction:\n  intermediate:\n    trigger: every_n_commits\n    n: 5\n    keep_recent: 4\n";
    assert_eq!(
        parse(ok)
            .unwrap()
            .compaction
            .unwrap()
            .intermediate
            .keep_recent,
        Some(4)
    );
    let time = "events: {}\ncompaction:\n  intermediate:\n    trigger: every_t_seconds\n    n: 5\n    keep_recent: 50\n";
    assert!(parse(time).is_ok());
}

#[test]
fn rejects_compaction_missing_n_for_count_trigger() {
    let yaml = "events: {}\ncompaction:\n  intermediate:\n    trigger: every_n_commits\n";
    let err = parse(yaml).unwrap_err();
    match err {
        LoadError::Invalid { key, .. } => assert_eq!(key, "compaction.intermediate.n"),
        other => panic!("expected Invalid, got {other:?}"),
    }
}

#[test]
fn rejects_compaction_missing_n_for_seconds_trigger() {
    let yaml = "events: {}\ncompaction:\n  intermediate:\n    trigger: every_t_seconds\n";
    assert!(matches!(parse(yaml), Err(LoadError::Invalid { .. })));
}

#[test]
fn on_flush_trigger_does_not_need_n() {
    let yaml = "events: {}\ncompaction:\n  intermediate:\n    trigger: on_flush\n";
    assert!(parse(yaml).is_ok());
}

#[test]
fn rejects_compaction_zero_n() {
    let yaml = "events: {}\ncompaction:\n  intermediate:\n    trigger: every_n_commits\n    n: 0\n";
    assert!(matches!(parse(yaml), Err(LoadError::Invalid { .. })));
}

#[test]
fn extract_bytes_is_optional_and_carries_the_landings_cap() {
    // `docs/DESIGN_CONTEXT_ECONOMY.md` §5.3: omitted means no extract
    // (severable, like `tool_output:`), and the number is bytes — the
    // unit a file in the tree has.
    let none = "events: {}\ncompaction:\n  intermediate:\n    trigger: on_flush\n";
    assert_eq!(
        parse(none)
            .unwrap()
            .compaction
            .unwrap()
            .intermediate
            .extract_bytes,
        None
    );
    let set = "events: {}\ncompaction:\n  intermediate:\n    trigger: on_flush\n    extract_bytes: 32768\n";
    assert_eq!(
        parse(set)
            .unwrap()
            .compaction
            .unwrap()
            .intermediate
            .extract_bytes,
        Some(32768)
    );
}

#[test]
fn window_percent_takes_n_as_a_percentage_and_refuses_the_rest() {
    // §5.1: `n` is a percent of the model's context window, so 1..=100 is
    // the whole lawful range. 0 is the shared missing-threshold decline
    // above; over 100 asks for a fraction no usage can reach — a trigger
    // that could never fire, which is what the variant exists to refuse.
    let at = |n: &str| {
        parse(&format!(
            "events: {{}}\ncompaction:\n  intermediate:\n    trigger: window_percent\n    n: {n}\n"
        ))
    };
    assert_eq!(
        at("100").unwrap().compaction.unwrap().intermediate.n,
        Some(100)
    );
    assert_eq!(at("1").unwrap().compaction.unwrap().intermediate.n, Some(1));
    match at("101").unwrap_err() {
        LoadError::Invalid { key, message, .. } => {
            assert_eq!(key, "compaction.intermediate.n");
            assert!(message.contains("1..=100"), "{message}");
        }
        other => panic!("expected Invalid, got {other:?}"),
    }
    // The threshold is required, like the other two clocked triggers.
    let missing = "events: {}\ncompaction:\n  intermediate:\n    trigger: window_percent\n";
    assert!(matches!(parse(missing), Err(LoadError::Invalid { .. })));
    // `keep_recent`'s must-stay-below-n rule is `every_n_commits`' alone:
    // under the window trigger the two numbers are in different units.
    let tail = "events: {}\ncompaction:\n  intermediate:\n    trigger: window_percent\n    n: 50\n    keep_recent: 90\n";
    assert!(parse(tail).is_ok());
}

#[test]
fn the_retained_tail_is_declared_once_in_one_unit() {
    // §5.2: `keep_recent` (commits) and `keep_recent_tokens` (the
    // provider's prompt tokens) are one fact in two units. Either alone
    // parses; both together are declined naming the key, rather than
    // resolved by a precedence nothing states.
    let both = "events: {}\ncompaction:\n  intermediate:\n    trigger: on_flush\n    keep_recent: 3\n    keep_recent_tokens: 20000\n";
    match parse(both).unwrap_err() {
        LoadError::Invalid { key, message, .. } => {
            assert_eq!(key, "compaction.intermediate.keep_recent_tokens");
            assert!(message.contains("keep_recent"), "{message}");
            assert!(message.contains("declare one"), "{message}");
        }
        other => panic!("expected Invalid, got {other:?}"),
    }
    let tokens = "events: {}\ncompaction:\n  intermediate:\n    trigger: on_flush\n    keep_recent_tokens: 20000\n";
    let c = parse(tokens).unwrap().compaction.unwrap();
    assert_eq!(c.intermediate.keep_recent_tokens, Some(20000));
    assert_eq!(c.intermediate.keep_recent, None);
    // A token tail is in the provider's unit, so `every_n_commits`'
    // must-stay-below-n rule — which exists because a commit count at or
    // over the commit clock re-arms every landing — does not reach it.
    let clocked = "events: {}\ncompaction:\n  intermediate:\n    trigger: every_n_commits\n    n: 5\n    keep_recent_tokens: 20000\n";
    assert!(parse(clocked).is_ok());
}
