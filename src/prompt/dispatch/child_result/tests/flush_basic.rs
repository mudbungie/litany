//! The `worker_flush` seam's basic cases (ARCH §2.7, §6): a due clock
//! forks a compactor, no `compaction:` block never fires, and a
//! non-dispatch binding there is declined. Split from [`cases`](super::cases)
//! along that seam to hold the per-file line cap; the clock's own cases
//! are [`flush_clock`](super::flush_clock).

use super::super::run_flush;
use super::{Fx, workflow};
use crate::config::version::Version;
use crate::prompt::Error;
use crate::workspace::fixture;

#[test]
fn run_flush_dispatches_a_compactor_when_the_checkpoint_is_due() {
    // §2.7/§6: a `compaction:` clock due at the boundary runs worker_flush
    // → dispatch(compactor), forking a compactor off the tip C.
    let (_h, ws) = fixture::workspace();
    let parent = "20260101-p6";
    let wt = fixture::spawn_root(&ws, parent);
    let fx = Fx::new();
    // every_n_commits n=1: the parent already has commits, so it is due.
    let wf = workflow(
        "events: {}\ncompaction:\n  intermediate:\n    trigger: every_n_commits\n    n: 1\n",
    );
    run_flush(&ws, parent, &wt, &wf, Version(2), &fx.deps()).unwrap();
    // A compactor child was launched through the front door.
    let launched = fx.launcher.launched.borrow();
    assert_eq!(launched.len(), 1);
    assert!(
        launched[0].starts_with(&format!("{parent}-")),
        "{launched:?}"
    );
}

#[test]
fn run_flush_is_a_noop_without_a_compaction_block() {
    let (_h, ws) = fixture::workspace();
    let parent = "20260101-p7";
    let wt = fixture::spawn_root(&ws, parent);
    let fx = Fx::new();
    run_flush(
        &ws,
        parent,
        &wt,
        &workflow("events: {}\n"),
        Version(2),
        &fx.deps(),
    )
    .unwrap();
    assert!(fx.launcher.launched.borrow().is_empty());
}

#[test]
fn run_flush_declines_an_unsupported_flush_action() {
    let (_h, ws) = fixture::workspace();
    let parent = "20260101-p8";
    let wt = fixture::spawn_root(&ws, parent);
    let fx = Fx::new();
    // Due, but worker_flush bound to a non-dispatch action → declined.
    let wf = workflow(
        "events:\n  worker_flush:\n    - notify_ui\ncompaction:\n  intermediate:\n    trigger: every_n_commits\n    n: 1\n",
    );
    let err = run_flush(&ws, parent, &wt, &wf, Version(2), &fx.deps()).unwrap_err();
    assert!(matches!(err, Error::ActionUnsupported { .. }), "{err:?}");
}
