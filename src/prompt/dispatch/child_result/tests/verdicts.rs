//! Role-generic verdict events (`docs/DESIGN_PROCEDURES.md` §5) over the
//! real-git harness: a bound token fires its event for the child's own
//! role, an unbound one is the plain return, the gate holds over any
//! role — and nothing approves anything: a worker saying `APPROVE` is a
//! worker's return, and a verifier nobody gates releases nothing.

use super::super::{has_pending_result, interpret_pending};
use super::{Fx, returned_child, returned_child_full, workflow};
use crate::prompt::inbox::{Epitaph, deposit_result};
use crate::prompt::{ChildDispatchRequest, child_dispatch};
use crate::template::GitRunner;
use crate::workspace::{agent_worktree, fixture};
use std::path::Path;

/// Fork `role` off `tip` (as the gate does) and deposit its `response`
/// into the parent's inbox as a finished return.
fn judged(ws: &Path, parent: &str, role: &str, tip: &str, response: &str, fx: &Fx) -> String {
    let parent_wt = agent_worktree(ws, parent);
    let req = ChildDispatchRequest {
        repo: ws,
        parent_branch: parent,
        parent_worktree: &parent_wt,
        role,
        goal: "judge",
        name: None,
        fork_point: Some(tip),
        cwd: None,
        pins: crate::prompt::PinnedDocs::none(),
    };
    let child = child_dispatch::run(
        &req,
        &fx.git,
        &fx.clock,
        &fx.id,
        &fx.launcher,
        crate::workspace::agent_name::mint::test_rng(),
    )
    .unwrap();
    let tip = fx
        .git
        .run_capture(&agent_worktree(ws, &child), &["rev-parse", "HEAD"])
        .unwrap();
    deposit_result(
        ws,
        parent,
        &child,
        Epitaph::FinalResponse,
        tip.trim(),
        Some(response),
        &fx.clock,
        &fx.git,
    )
    .unwrap();
    child
}

fn tip_of(ws: &Path, agent: &str, fx: &Fx) -> String {
    fx.git
        .run_capture(&agent_worktree(ws, agent), &["rev-parse", "HEAD"])
        .unwrap()
        .trim()
        .to_string()
}

fn transcript_entries(wt: &Path) -> usize {
    std::fs::read_dir(wt.join("messages")).map_or(0, |d| d.count())
}

#[test]
fn a_bound_token_fires_the_roles_verdict_event() {
    let (_h, ws) = fixture::workspace();
    fixture::amend_config(&ws, &[("souls/classifier.md", "c")]);
    let parent = "20260101-v1";
    let wt = fixture::spawn_root(&ws, parent);
    let fx = Fx::new();
    returned_child_full(
        &ws,
        parent,
        "classifier",
        "sort it",
        ("o.txt", "x"),
        Epitaph::FinalResponse,
        Some("SWITCH: this is a planning conversation"),
        &fx,
    );
    let wf = workflow("events:\n  classifier_switch:\n    - land_response(verdict)\n");
    interpret_pending(&ws, parent, &wt, &wf, &fx.deps()).unwrap();
    assert_eq!(
        std::fs::read_to_string(wt.join("verdict")).unwrap(),
        "SWITCH: this is a planning conversation\n"
    );
    assert_eq!(transcript_entries(&wt), 0, "landed, not delivered");
}

#[test]
fn an_unbound_token_is_the_plain_return() {
    // A token nobody bound is no event: the same response under a
    // workflow that binds nothing for it delivers as `classifier_return`.
    let (_h, ws) = fixture::workspace();
    fixture::amend_config(&ws, &[("souls/classifier.md", "c")]);
    let parent = "20260101-v2";
    let wt = fixture::spawn_root(&ws, parent);
    let fx = Fx::new();
    returned_child_full(
        &ws,
        parent,
        "classifier",
        "sort it",
        ("o.txt", "x"),
        Epitaph::FinalResponse,
        Some("SWITCH now"),
        &fx,
    );
    let wf = workflow("events:\n  reviewer_switch:\n    - land_response(verdict)\n");
    interpret_pending(&ws, parent, &wt, &wf, &fx.deps()).unwrap();
    assert!(
        !wt.join("verdict").exists(),
        "another role's binding is not this role's"
    );
    assert_eq!(transcript_entries(&wt), 1);
    assert!(wt.join("o.txt").exists(), "an ordinary return transfers");
}

#[test]
fn a_worker_saying_approve_is_a_workers_return_whatever_is_bound() {
    // The role is the harness's fact, not the child's claim: a worker's
    // `APPROVE` fires `worker_approve`, which nothing binds, so it is the
    // work it is — even with the verifier's verdict bound right there.
    let (_h, ws) = fixture::workspace();
    let parent = "20260101-v3";
    let wt = fixture::spawn_root(&ws, parent);
    let fx = Fx::new();
    returned_child_full(
        &ws,
        parent,
        "worker",
        "do it",
        ("o.txt", "x"),
        Epitaph::FinalResponse,
        Some("APPROVE"),
        &fx,
    );
    let wf = workflow("events:\n  verifier_approve:\n    - land_response(approved)\n");
    interpret_pending(&ws, parent, &wt, &wf, &fx.deps()).unwrap();
    assert!(!wt.join("approved").exists());
    assert_eq!(transcript_entries(&wt), 1);
}

#[test]
fn a_verifier_nobody_gates_releases_nothing() {
    // A verifier the model dispatched by hand (off the parent's tip, not
    // off a held result) is no gating child: its `APPROVE` is a bound
    // verdict, and the verdict's `deliver_result` delivers *its own*
    // return — a pending worker result it never gated stays exactly as
    // it was.
    let (_h, ws) = fixture::workspace();
    fixture::amend_config(&ws, &[("souls/verifier.md", "v")]);
    let parent = "20260101-v4";
    let wt = fixture::spawn_root(&ws, parent);
    let fx = Fx::new();
    let worker = returned_child(&ws, parent, "worker", "do it", ("o.txt", "x"), &fx);
    let verifier = judged(
        &ws,
        parent,
        "verifier",
        &tip_of(&ws, parent, &fx),
        "APPROVE",
        &fx,
    );
    let wf = workflow(
        "events:\n  worker_return:\n    - gate_return_on(verifier.approve)\n  verifier_approve:\n    - deliver_result\n",
    );
    interpret_pending(&ws, parent, &wt, &wf, &fx.deps()).unwrap();
    assert!(
        wt.join(format!("messages/001-{verifier}.md")).exists(),
        "the verifier's own return delivered"
    );
    assert!(
        !wt.join(format!("messages/002-{worker}.md")).exists() && !wt.join("o.txt").exists(),
        "the worker result it never gated is not released"
    );
    assert!(has_pending_result(&ws, parent).unwrap());
}

#[test]
fn the_gate_holds_over_any_role_and_its_unmet_outcome_redispatches() {
    // `dispatch(reviewer)` + `gate_return_on(reviewer.lgtm)` on a worker's
    // return: the reviewer forks off the worker's tip; `LGTM` lifts the
    // hold, anything else is `reviewer_reject`, whose baseline
    // re-dispatches the gated role with the verdict as feedback.
    let (_h, ws) = fixture::workspace();
    let parent = "20260101-v5";
    let wt = fixture::spawn_root(&ws, parent);
    let fx = Fx::new();
    let wf = workflow(
        "events:\n  worker_return:\n    - dispatch(reviewer)\n    - gate_return_on(reviewer.lgtm)\n",
    );
    let worker = returned_child(&ws, parent, "worker", "do it", ("o.txt", "x"), &fx);
    interpret_pending(&ws, parent, &wt, &wf, &fx.deps()).unwrap();
    assert!(has_pending_result(&ws, parent).unwrap(), "held");
    let reviewer = fx.launcher.launched.borrow()[1].clone();
    let goal = std::fs::read_to_string(agent_worktree(&ws, &reviewer).join("goal.md")).unwrap();
    assert!(goal.contains("You are the reviewer for branch"), "{goal}");
    assert!(
        goal.contains("done"),
        "the returning response rides the goal: {goal}"
    );

    // Unmet: the reviewer says something else.
    judged(
        &ws,
        parent,
        "reviewer",
        &tip_of(&ws, &worker, &fx),
        "NOPE, missing tests",
        &fx,
    );
    interpret_pending(&ws, parent, &wt, &wf, &fx.deps()).unwrap();
    assert!(!has_pending_result(&ws, parent).unwrap(), "both consumed");
    let redispatched = fx.launcher.launched.borrow().last().cloned().unwrap();
    let goal = std::fs::read_to_string(agent_worktree(&ws, &redispatched).join("goal.md")).unwrap();
    assert_eq!(goal, "NOPE, missing tests");
    assert_eq!(transcript_entries(&wt), 0);

    // Met: a second worker, gated, and a reviewer that says LGTM.
    let worker2 = returned_child(&ws, parent, "worker", "do it again", ("p.txt", "y"), &fx);
    interpret_pending(&ws, parent, &wt, &wf, &fx.deps()).unwrap();
    judged(
        &ws,
        parent,
        "reviewer",
        &tip_of(&ws, &worker2, &fx),
        "lgtm — ship it",
        &fx,
    );
    interpret_pending(&ws, parent, &wt, &wf, &fx.deps()).unwrap();
    assert!(wt.join("p.txt").exists(), "the held result delivered");
    assert!(wt.join(format!("messages/001-{worker2}.md")).exists());
    assert!(!has_pending_result(&ws, parent).unwrap());
}

#[test]
fn an_unmet_gate_with_a_bare_dispatch_steers_with_the_verdict() {
    // `<role>_reject: [dispatch(worker)]` with no `with:` at all: the
    // re-dispatched worker's goal is the verdict itself, exactly as
    // under the `<role>.feedback` token — the absent arm is the same
    // answer, not a third.
    let (_h, ws) = fixture::workspace();
    let parent = "20260101-v6";
    let wt = fixture::spawn_root(&ws, parent);
    let fx = Fx::new();
    let wf = workflow(
        "events:\n  worker_return:\n    - dispatch(reviewer)\n    - gate_return_on(reviewer.lgtm)\n  reviewer_reject:\n    - dispatch(worker)\n",
    );
    let worker = returned_child(&ws, parent, "worker", "do it", ("o.txt", "x"), &fx);
    interpret_pending(&ws, parent, &wt, &wf, &fx.deps()).unwrap();
    judged(
        &ws,
        parent,
        "reviewer",
        &tip_of(&ws, &worker, &fx),
        "NOPE",
        &fx,
    );
    interpret_pending(&ws, parent, &wt, &wf, &fx.deps()).unwrap();
    let redispatched = fx.launcher.launched.borrow().last().cloned().unwrap();
    let goal = std::fs::read_to_string(agent_worktree(&ws, &redispatched).join("goal.md")).unwrap();
    assert_eq!(goal, "NOPE");
}
