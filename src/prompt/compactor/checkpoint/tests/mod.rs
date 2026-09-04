//! Tests for checkpoint trigger evaluation (ARCH §6).
//!
//! [`due`] is pure — driven by constructed states, and that is this
//! module. [`state`]'s git derivation runs against a real repo in
//! [`derive`], so the origin grep and the commit-count / elapsed-time
//! measures are exercised end-to-end.

use super::*;
use crate::config::workflow::{CompactionConfig, compaction::IntermediateCompaction};

mod derive;

fn cfg(trigger: CompactionTrigger, n: Option<u32>) -> CompactionConfig {
    CompactionConfig {
        intermediate: IntermediateCompaction {
            trigger,
            n,
            keep_recent: None,
            keep_recent_tokens: None,
            extract_bytes: None,
        },
    }
}

fn st(commits: u32, seconds: u64, flush: bool) -> CheckpointState {
    CheckpointState {
        commits_since_checkpoint: commits,
        seconds_since_checkpoint: seconds,
        flush_requested: flush,
        is_compactor: false,
        compaction_in_flight: false,
        last_usage: None,
    }
}

#[test]
fn no_config_never_compacts() {
    assert!(!due(None, &st(1000, 1000, true)).unwrap());
}

#[test]
fn every_n_commits_fires_at_or_past_the_threshold() {
    let c = cfg(CompactionTrigger::EveryNCommits, Some(3));
    assert!(!due(Some(&c), &st(2, 0, false)).unwrap());
    assert!(due(Some(&c), &st(3, 0, false)).unwrap());
    assert!(due(Some(&c), &st(4, 0, false)).unwrap());
}

#[test]
fn every_t_seconds_fires_at_or_past_the_threshold() {
    let c = cfg(CompactionTrigger::EveryTSeconds, Some(10));
    assert!(!due(Some(&c), &st(0, 9, false)).unwrap());
    assert!(due(Some(&c), &st(0, 10, false)).unwrap());
}

#[test]
fn on_flush_fires_only_when_the_agent_elects_it() {
    let c = cfg(CompactionTrigger::OnFlush, None);
    assert!(!due(Some(&c), &st(9999, 9999, false)).unwrap());
    assert!(due(Some(&c), &st(0, 0, true)).unwrap());
}

#[test]
fn a_malformed_threshold_fails_closed() {
    // n absent or zero (guarded at config load, §6) is never due — a bad
    // config does not compact every step.
    assert!(
        !due(
            Some(&cfg(CompactionTrigger::EveryNCommits, None)),
            &st(100, 0, false)
        )
        .unwrap()
    );
    assert!(
        !due(
            Some(&cfg(CompactionTrigger::EveryTSeconds, Some(0))),
            &st(0, 100, false)
        )
        .unwrap()
    );
}

#[test]
fn a_compactor_is_never_compaction_eligible() {
    // The invariant: a compactor *is* the compaction, not a subject of
    // one (§2.7). No trigger, at any count/elapsed/elected flush, admits
    // it to the eligible set — this is what stops a compactor from
    // dispatching a compactor (bl-a9eb / yog bl-ebbd).
    let compactor = CheckpointState {
        is_compactor: true,
        ..st(9999, 9999, true)
    };
    for c in [
        cfg(CompactionTrigger::EveryNCommits, Some(1)),
        cfg(CompactionTrigger::EveryTSeconds, Some(1)),
        cfg(CompactionTrigger::OnFlush, None),
    ] {
        assert!(
            !due(Some(&c), &compactor).unwrap(),
            "{:?}",
            c.intermediate.trigger
        );
        // The same state on a non-compactor branch *is* due, so the
        // exclusion is the only thing suppressing it.
        assert!(due(Some(&c), &st(9999, 9999, true)).unwrap());
    }
}

#[test]
fn a_branch_with_a_compaction_in_flight_is_never_due() {
    // bl-b9f0: the checkpoint this branch is standing on has already
    // fired and its answer has not come back. Firing again buys a
    // second full model loop over the same span that the landing then
    // refuses as superseded (§2.6) — so the suppression holds under
    // every trigger, the agent-elected flush included, exactly as the
    // compactor exclusion does.
    let waiting = CheckpointState {
        compaction_in_flight: true,
        ..st(9999, 9999, true)
    };
    for c in [
        cfg(CompactionTrigger::EveryNCommits, Some(1)),
        cfg(CompactionTrigger::EveryTSeconds, Some(1)),
        cfg(CompactionTrigger::OnFlush, None),
    ] {
        assert!(
            !due(Some(&c), &waiting).unwrap(),
            "{:?}",
            c.intermediate.trigger
        );
        // The same branch with nothing in flight *is* due, so the
        // suppressor is the only thing holding it.
        assert!(due(Some(&c), &st(9999, 9999, true)).unwrap());
    }
}

#[test]
fn window_percent_routes_through_the_last_usage() {
    // The trigger's whole answer is [`usage::due`]'s (§5.1); this pins
    // that `due` dispatches to it — and that no config-clock field
    // (commits, elapsed, elected flush) moves it.
    let c = cfg(CompactionTrigger::WindowPercent, Some(50));
    let filled = |prompt| CheckpointState {
        last_usage: Some(LastUsage {
            prompt_tokens: prompt,
            context_window: Some(200),
            model: "m".into(),
        }),
        ..st(9999, 9999, false)
    };
    assert!(!due(Some(&c), &filled(99)).unwrap());
    assert!(due(Some(&c), &filled(100)).unwrap());
    // No model entry yet: not due, and no decline — an absent report is
    // not an unknown window.
    assert!(!due(Some(&c), &st(9999, 9999, false)).unwrap());
}

#[test]
fn the_two_suppressors_answer_ahead_of_the_windows_decline() {
    // A compactor and a branch with a compaction in flight are excluded
    // under *every* trigger (module docs), so neither reaches the
    // window's unknown-window decline: the answer is "not due", not an
    // error. Otherwise a `window_percent` workspace could not compact at
    // all — every compactor it dispatched would abort its own boundary.
    let c = cfg(CompactionTrigger::WindowPercent, Some(50));
    let blind = LastUsage {
        prompt_tokens: 9999,
        context_window: None,
        model: "m".into(),
    };
    for excluded in [
        CheckpointState {
            is_compactor: true,
            last_usage: Some(blind.clone()),
            ..st(0, 0, false)
        },
        CheckpointState {
            compaction_in_flight: true,
            last_usage: Some(blind.clone()),
            ..st(0, 0, false)
        },
    ] {
        assert!(!due(Some(&c), &excluded).unwrap());
    }
    // The same state with neither suppressor *is* the decline, so the
    // exclusions are the only thing holding it.
    let reached = CheckpointState {
        last_usage: Some(blind),
        ..st(0, 0, false)
    };
    assert!(due(Some(&c), &reached).is_err());
}
