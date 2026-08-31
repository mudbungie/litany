//! **A compaction in flight suppresses the next checkpoint** (ARCH §2.7,
//! the third eligibility invariant; bl-b9f0) — driven at the seam that
//! dispatches, `run_flush`. Split from [`cases`](super::cases) to hold
//! the per-file line cap; the shared real-git harness lives in
//! [`super`].

use super::super::run_flush;
use super::{Fx, workflow};
use crate::template::GitRunner;
use crate::workspace::fixture;

#[test]
fn a_compaction_in_flight_suppresses_the_next_checkpoint() {
    // bl-b9f0, the live repro: the boundary after step 010 dispatched a
    // compactor, the boundary after step 011 — eight seconds later —
    // dispatched a second off the same span, because a compaction that
    // has been dispatched has landed no base and the clock reads the
    // same count. Two boundaries here, one compactor.
    let (_h, ws) = fixture::workspace();
    let parent = "20260101-pa";
    let wt = fixture::spawn_root(&ws, parent);
    let fx = Fx::new();
    let wf = workflow(
        "events: {}\ncompaction:\n  intermediate:\n    trigger: every_n_commits\n    n: 1\n",
    );
    run_flush(&ws, parent, &wt, &wf, &fx.deps()).unwrap();
    run_flush(&ws, parent, &wt, &wf, &fx.deps()).unwrap();
    let launched = fx.launcher.launched.borrow();
    assert_eq!(
        launched.len(),
        1,
        "one pass in flight, one compactor: {launched:?}"
    );

    // And the suppression lifts on the fact that ends the pass, not on
    // a timer: the returned mark every result deposit writes.
    let child = launched[0].clone();
    drop(launched);
    let mark = crate::prompt::inbox::deposit::returned_ref(&child);
    let tip = fx.git.run_capture(&wt, &["rev-parse", "HEAD"]).unwrap();
    fx.git.run(&wt, &["update-ref", &mark, tip.trim()]).unwrap();
    run_flush(&ws, parent, &wt, &wf, &fx.deps()).unwrap();
    assert_eq!(
        fx.launcher.launched.borrow().len(),
        2,
        "a returned compactor no longer suppresses"
    );
}

#[test]
fn a_worker_child_still_in_flight_suppresses_nothing() {
    // The suppressor is keyed on the child's ROLE, read off its dispatch
    // commit subject (the one home): an ordinary child this branch is
    // waiting on is not a compaction and must not hold the clock.
    let (_h, ws) = fixture::workspace();
    let parent = "20260101-pb";
    let wt = fixture::spawn_root(&ws, parent);
    let fx = Fx::new();
    crate::prompt::child_dispatch::run(
        &crate::prompt::ChildDispatchRequest {
            repo: &ws,
            parent_branch: parent,
            parent_worktree: &wt,
            role: "worker",
            goal: "do the thing",
            name: None,
            fork_point: None,
            cwd: None,
            pins: crate::prompt::PinnedDocs::none(),
        },
        &fx.git,
        &fx.clock,
        &fx.id,
        &fx.launcher,
        crate::workspace::agent_name::mint::test_rng(),
    )
    .unwrap();
    let wf = workflow(
        "events: {}\ncompaction:\n  intermediate:\n    trigger: every_n_commits\n    n: 1\n",
    );
    run_flush(&ws, parent, &wt, &wf, &fx.deps()).unwrap();
    assert_eq!(
        fx.launcher.launched.borrow().len(),
        2,
        "the worker's own launch, then the compactor's"
    );
}
