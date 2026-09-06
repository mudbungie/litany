//! Happy-path orchestration tests for [`crate::prompt::stop::run`].

use super::fixtures::{NoopGit, StubFinder, StubInspector, touch_inbox_dir};
use crate::prompt::stop::{Error, cascade, run};
use std::time::Duration;
use tempfile::TempDir;

/// SIGTERM targets recorded by the signaler, sorted for order-free
/// assertion.
fn term_targets(signaler: &cascade::RecordingSignaler) -> Vec<i32> {
    let mut t: Vec<i32> = signaler
        .took()
        .into_iter()
        .filter(|(s, _)| *s == "term")
        .map(|(_, target)| target)
        .collect();
    t.sort();
    t
}

#[test]
fn run_returns_branch_missing_when_inspector_says_no() {
    let dir = TempDir::new().unwrap();
    let inspector = StubInspector { exists: false };
    let err = run(
        dir.path(),
        "br",
        &inspector,
        &StubFinder::default(),
        &cascade::RecordingSignaler::new(0),
        Duration::from_millis(1),
        &NoopGit,
    )
    .unwrap_err();
    matches!(err, Error::BranchMissing(b) if b == "br");
}

#[test]
fn run_idempotent_when_no_holder_found() {
    let dir = TempDir::new().unwrap();
    touch_inbox_dir(dir.path(), "br");
    let inspector = StubInspector { exists: true };
    let signaler = cascade::RecordingSignaler::new(0);
    run(
        dir.path(),
        "br",
        &inspector,
        &StubFinder::default(),
        &signaler,
        Duration::from_millis(1),
        &NoopGit,
    )
    .unwrap();
    assert!(
        signaler.took().is_empty(),
        "no lock holder → no signals sent (idempotent)"
    );
}

#[test]
fn run_idempotent_when_no_inbox_dir_at_all() {
    let dir = TempDir::new().unwrap();
    let inspector = StubInspector { exists: true };
    let signaler = cascade::RecordingSignaler::new(0);
    run(
        dir.path(),
        "br",
        &inspector,
        &StubFinder::default(),
        &signaler,
        Duration::from_millis(1),
        &NoopGit,
    )
    .unwrap();
    assert!(signaler.took().is_empty());
}

#[test]
fn run_signals_the_inbox_dir_holder() {
    let dir = TempDir::new().unwrap();
    let inbox = touch_inbox_dir(dir.path(), "br");
    let inspector = StubInspector { exists: true };
    let finder = StubFinder::with_returns(vec![Some(123)]);
    let signaler = cascade::RecordingSignaler::new(0);
    run(
        dir.path(),
        "br",
        &inspector,
        &finder,
        &signaler,
        Duration::from_millis(1),
        &NoopGit,
    )
    .unwrap();
    assert_eq!(finder.seen.lock().unwrap().as_slice(), &[inbox]);
    assert_eq!(signaler.took(), vec![("term", 123)]);
}

#[test]
fn run_reaches_a_live_child_with_no_flag_asked_for() {
    // The regression bl-3114 filed, from the collector's other end: the
    // same on-disk tree that used to leave `br-sub` running now probes
    // its inbox too, so the descendant executor is in the sweep. A stop
    // that left it alive left the conversation spending, because the
    // child's `final-response` return revives its dispatcher (§2.11
    // pin 2).
    let dir = TempDir::new().unwrap();
    let self_inbox = touch_inbox_dir(dir.path(), "br");
    let child_inbox = touch_inbox_dir(dir.path(), "br-sub");
    let inspector = StubInspector { exists: true };
    let finder = StubFinder::with_returns(vec![Some(11), Some(12)]);
    let signaler = cascade::RecordingSignaler::new(0);
    run(
        dir.path(),
        "br",
        &inspector,
        &finder,
        &signaler,
        Duration::from_millis(1),
        &NoopGit,
    )
    .unwrap();
    let mut probed = finder.seen.lock().unwrap().clone();
    probed.sort();
    let mut expected = vec![self_inbox, child_inbox];
    expected.sort();
    assert_eq!(probed, expected, "both inboxes probed");
    assert_eq!(term_targets(&signaler), vec![11, 12]);
}

#[test]
fn run_covers_descended_subagent_ids_via_hyphen_prefix() {
    // The walk over the id namespace: `br` plus every `br-*`
    // descendant, each its own executor pgid, folded into one
    // sweep (§2.9).
    let dir = TempDir::new().unwrap();
    touch_inbox_dir(dir.path(), "br");
    touch_inbox_dir(dir.path(), "br-sub");
    let inspector = StubInspector { exists: true };
    let finder = StubFinder::with_returns(vec![Some(11), Some(22)]);
    let signaler = cascade::RecordingSignaler::new(0);
    run(
        dir.path(),
        "br",
        &inspector,
        &finder,
        &signaler,
        Duration::from_millis(1),
        &NoopGit,
    )
    .unwrap();
    assert_eq!(term_targets(&signaler), vec![11, 22]);
}

#[test]
fn run_covers_deep_descendants_in_one_prefix_scan() {
    // The flat id namespace *is* the tree: a grandchild `br-a-b` is
    // prefixed `br-` too, so one scan reaches every depth — no
    // recursion (§2.9).
    let dir = TempDir::new().unwrap();
    touch_inbox_dir(dir.path(), "br");
    touch_inbox_dir(dir.path(), "br-a");
    touch_inbox_dir(dir.path(), "br-a-b");
    let inspector = StubInspector { exists: true };
    let finder = StubFinder::with_returns(vec![Some(1), Some(2), Some(3)]);
    let signaler = cascade::RecordingSignaler::new(0);
    run(
        dir.path(),
        "br",
        &inspector,
        &finder,
        &signaler,
        Duration::from_millis(1),
        &NoopGit,
    )
    .unwrap();
    assert_eq!(term_targets(&signaler), vec![1, 2, 3]);
}

#[test]
fn run_dedupes_pgid_when_multiple_holders_share_one() {
    let dir = TempDir::new().unwrap();
    touch_inbox_dir(dir.path(), "br");
    touch_inbox_dir(dir.path(), "br-sub");
    let inspector = StubInspector { exists: true };
    let finder = StubFinder::with_returns(vec![Some(7), Some(7)]);
    let signaler = cascade::RecordingSignaler::new(0);
    run(
        dir.path(),
        "br",
        &inspector,
        &finder,
        &signaler,
        Duration::from_millis(1),
        &NoopGit,
    )
    .unwrap();
    assert_eq!(signaler.took(), vec![("term", 7)]);
}

#[test]
fn run_skips_unrelated_agent_id_dirs() {
    let dir = TempDir::new().unwrap();
    touch_inbox_dir(dir.path(), "br");
    touch_inbox_dir(dir.path(), "different");
    touch_inbox_dir(dir.path(), "br2-not-descended");
    let inspector = StubInspector { exists: true };
    let finder = StubFinder::with_returns(vec![Some(99)]);
    let signaler = cascade::RecordingSignaler::new(0);
    run(
        dir.path(),
        "br",
        &inspector,
        &finder,
        &signaler,
        Duration::from_millis(1),
        &NoopGit,
    )
    .unwrap();
    let seen = finder.seen.lock().unwrap().clone();
    assert_eq!(seen.len(), 1, "only `br` should match: got {seen:?}");
    assert!(seen[0].to_string_lossy().contains("/inbox/br"));
}
