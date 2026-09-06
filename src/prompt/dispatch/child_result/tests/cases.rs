//! The §6 delivered-child-result and checkpoint-flush test cases. The
//! shared real-git harness (stubs, `Fx`, `returned_child`) lives in the
//! parent [`super`] module.

use super::super::{has_pending_result, interpret_pending, run_flush};
use super::{Fx, returned_child, returned_child_ep, steer, workflow};
use crate::prompt::inbox::Epitaph;
use crate::prompt::{Error, SystemClock};
use crate::template::GitRunner;
use crate::workspace::{agent_worktree, fixture};

#[test]
fn a_worker_result_delivers_by_default_transfer_plus_transcript() {
    // §6 worker_return baseline (unbound → deliver_result): the child's
    // work product transfers to the parent tree and its result message
    // lands in the transcript; the inbox file is consumed.
    let (_h, ws) = fixture::workspace();
    let parent = "20260101-p1";
    fixture::spawn_root(&ws, parent);
    let fx = Fx::new();
    let child = returned_child(&ws, parent, "worker", "do it", ("out.txt", "result\n"), &fx);

    let wt = agent_worktree(&ws, parent);
    interpret_pending(&ws, parent, &wt, &workflow("events: {}\n"), &fx.deps()).unwrap();

    assert_eq!(
        std::fs::read_to_string(wt.join("out.txt")).unwrap(),
        "result\n"
    );
    let delivered = wt.join(format!("messages/001-{child}.md"));
    assert!(delivered.exists(), "result message delivered to transcript");
    assert!(!has_pending_result(&ws, parent).unwrap(), "inbox consumed");
}

#[test]
fn a_compactor_result_lands_the_compaction_and_consumes_the_message() {
    // §6 compactor_return baseline (unbound → land_compaction): the
    // compactor's summary lands on the parent branch as the compaction
    // base, and the trigger message is removed (not delivered — the base
    // commit is the record).
    let (_h, ws) = fixture::workspace();
    let parent = "20260101-p2";
    fixture::spawn_root(&ws, parent);
    let fx = Fx::new();
    let child = returned_child(
        &ws,
        parent,
        "compactor",
        "compact",
        ("summary/001.md", "sum\n"),
        &fx,
    );

    let wt = agent_worktree(&ws, parent);
    interpret_pending(&ws, parent, &wt, &workflow("events: {}\n"), &fx.deps()).unwrap();

    assert_eq!(
        std::fs::read_to_string(wt.join("summary/001.md")).unwrap(),
        "sum\n"
    );
    // The compaction base landed on the parent branch — an ordinary
    // single-parent commit, not a merge (§2.6 rebase-forward): with no
    // live commits past the compaction point, the replay is empty and
    // the branch tip *is* the base.
    let subj = fx
        .git
        .run_capture(&wt, &["log", "-1", "--format=%s"])
        .unwrap();
    assert!(
        subj.contains(&format!("compaction base [{child}]")),
        "{subj}"
    );
    let parents = fx
        .git
        .run_capture(&wt, &["rev-list", "--parents", "-n", "1", "HEAD"])
        .unwrap();
    assert_eq!(
        parents.split_whitespace().count(),
        2,
        "one parent — nothing merges anywhere (§2.6): {parents}"
    );
    assert!(
        !has_pending_result(&ws, parent).unwrap(),
        "trigger consumed"
    );
    // No transcript delivery for the compactor's result.
    assert!(!wt.join(format!("messages/001-{child}.md")).exists());
}

#[test]
fn a_died_compactor_return_lands_nothing_and_delivers_the_epitaph() {
    // §2.6/§2.7 epitaph gate: a compactor ending on any epitaph but
    // `final-response` lands NOTHING (its branch may hold a partial
    // pass); its result delivers like an ordinary child return instead.
    let (_h, ws) = fixture::workspace();
    let parent = "20260101-p9";
    fixture::spawn_root(&ws, parent);
    let fx = Fx::new();
    let child = returned_child_ep(
        &ws,
        parent,
        "compactor",
        "compact",
        ("summary/001.md", "partial\n"),
        Epitaph::Died,
        &fx,
    );

    let wt = agent_worktree(&ws, parent);
    interpret_pending(&ws, parent, &wt, &workflow("events: {}\n"), &fx.deps()).unwrap();

    // Nothing landed; the compactor's tree never crossed (§2.6).
    let log = fx.git.run_capture(&wt, &["log", "--format=%s"]).unwrap();
    assert!(!log.contains("compaction base"), "{log}");
    assert!(!wt.join("summary/001.md").exists(), "no compactor tree");
    // Delivered as an ordinary child return: the epitaph is reviewable
    // in the parent's transcript (§2.7) and the inbox is drained.
    let delivered = wt.join(format!("messages/001-{child}.md"));
    let body = std::fs::read_to_string(&delivered).unwrap();
    assert!(body.contains("epitaph: died"), "{body}");
    assert!(!has_pending_result(&ws, parent).unwrap(), "inbox consumed");
}

#[test]
fn a_stopped_compactor_return_lands_nothing_under_an_explicit_binding() {
    // The gate is on the action, not the default: an explicit
    // `compactor_return: compaction_merge` binding — the retired spelling,
    // still parsing to `land_compaction` (frozen configs keep resolving) —
    // is equally gated, so no workflow config can land a compactor that
    // did not finish.
    let (_h, ws) = fixture::workspace();
    let parent = "20260101-pa";
    fixture::spawn_root(&ws, parent);
    let fx = Fx::new();
    let work = ("summary/001.md", "partial\n");
    let child = returned_child_ep(
        &ws,
        parent,
        "compactor",
        "compact",
        work,
        Epitaph::Stopped,
        &fx,
    );
    let wt = agent_worktree(&ws, parent);
    let wf = workflow("events:\n  compactor_return:\n    - compaction_merge\n");
    interpret_pending(&ws, parent, &wt, &wf, &fx.deps()).unwrap();
    let log = fx.git.run_capture(&wt, &["log", "--format=%s"]).unwrap();
    assert!(!log.contains("compaction base"), "{log}");
    assert!(wt.join(format!("messages/001-{child}.md")).exists());
}

#[test]
fn an_explicit_worker_return_binding_is_honored() {
    let (_h, ws) = fixture::workspace();
    let parent = "20260101-p3";
    fixture::spawn_root(&ws, parent);
    let fx = Fx::new();
    let child = returned_child(&ws, parent, "worker", "do it", ("out.txt", "x\n"), &fx);
    let wt = agent_worktree(&ws, parent);
    let wf = workflow("events:\n  worker_return:\n    - deliver_result\n");
    interpret_pending(&ws, parent, &wt, &wf, &fx.deps()).unwrap();
    assert!(wt.join(format!("messages/001-{child}.md")).exists());
}

#[test]
fn an_unsupported_child_result_action_is_declined_loudly() {
    let (_h, ws) = fixture::workspace();
    let parent = "20260101-p4";
    fixture::spawn_root(&ws, parent);
    let fx = Fx::new();
    returned_child(&ws, parent, "worker", "do it", ("out.txt", "x\n"), &fx);
    let wt = agent_worktree(&ws, parent);
    // A worker_return bound to an action with no child-result executor
    // (a ref-mark action belongs to the branch's own terminal) is declined
    // here, never silently no-oped.
    let wf = workflow("events:\n  worker_return:\n    - notify_ui\n");
    let err = interpret_pending(&ws, parent, &wt, &wf, &fx.deps()).unwrap_err();
    assert!(matches!(err, Error::ActionUnsupported { .. }), "{err:?}");
}

#[test]
fn has_pending_result_is_false_without_a_result_message() {
    let (_h, ws) = fixture::workspace();
    let parent = "20260101-p5";
    fixture::spawn_root(&ws, parent);
    // An ordinary steering deposit is not a result message.
    steer(&ws, parent);
    assert!(!has_pending_result(&ws, parent).unwrap());
}

#[test]
fn interpret_pending_skips_a_steering_deposit() {
    // The same steering-vs-result split inside the interpreter itself:
    // a deposit with no `terminal_ref:` is not a child result, so the
    // interpreter loads nothing and touches no dep (all of Fx's deps
    // are unreachable stubs — reaching one would panic).
    let (_h, ws) = fixture::workspace();
    let parent = "20260101-p8";
    let wt = fixture::spawn_root(&ws, parent);
    steer(&ws, parent);
    let fx = Fx::new();
    let wf = workflow("events: {}\n");
    interpret_pending(&ws, parent, &wt, &wf, &fx.deps()).unwrap();
    // The steering message is left where it was deposited, undrained.
    assert!(!has_pending_result(&ws, parent).unwrap());
}

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
    run_flush(&ws, parent, &wt, &wf, &fx.deps()).unwrap();
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
    run_flush(&ws, parent, &wt, &workflow("events: {}\n"), &fx.deps()).unwrap();
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
    let err = run_flush(&ws, parent, &wt, &wf, &fx.deps()).unwrap_err();
    assert!(matches!(err, Error::ActionUnsupported { .. }), "{err:?}");
}

#[test]
fn a_reply_from_an_agent_this_one_never_dispatched_is_not_a_child_result() {
    // §2.6: the return is the *dispatcher's* business — the work-product
    // transfer diffs against the fork the dispatcher made, and the §6
    // bindings act on a child it dispatched. A reply carrying the same
    // frontmatter from an agent under another lineage has neither
    // relationship, so the interpreter does not see it at all; the drain
    // delivers it as the ordinary message it is.
    let (_h, ws) = fixture::workspace();
    let parent = "20260101-p9";
    fixture::spawn_root(&ws, parent);
    let fx = Fx::new();
    // A nephew by the id arithmetic: its dispatcher is `20260101-p9-x1`,
    // not `parent`.
    let stranger = "20260101-p9-x1-20260102-y2";
    let wt = agent_worktree(&ws, parent);
    let tip = fx.git.run_capture(&wt, &["rev-parse", "HEAD"]).unwrap();
    crate::prompt::inbox::deposit_result(
        &ws,
        parent,
        stranger,
        Epitaph::FinalResponse,
        tip.trim(),
        Some("hi"),
        &SystemClock,
        &fx.git,
    )
    .unwrap();

    assert!(
        !has_pending_result(&ws, parent).unwrap(),
        "not a circumstance the §6 interpreter answers to"
    );
    interpret_pending(&ws, parent, &wt, &workflow("events: {}\n"), &fx.deps()).unwrap();
    assert!(
        crate::prompt::inbox::inbox_dir(&ws, parent)
            .join(format!("{stranger}-001.md"))
            .exists(),
        "left for the drain, untouched"
    );
    assert!(!wt.join(format!("messages/001-{stranger}.md")).exists());
}
