//! Revival-on-deposit at the **parent** (ARCH §2.11, §2.5): a child's
//! terminal result deposit must start a driver at the parent, so a quiescent
//! parent wakes, delivers, and steps with no `litany scan` in the path.
//!
//! The launch rides the writer's post-deposit probe — the seam `litany
//! message` runs ([`crate::prompt::inbox::probe_and_launch`]) — so a
//! parent whose lease is held gets nothing (its own executor drains at
//! its next boundary), and §2.11 pin 2's epitaph decision governs it as
//! it governs the self-directed launch: a `stopped` or `budget-exhausted`
//! child deposits and wakes nobody. [`super::exit_launch`] covers the same
//! rules on the root step loop's terminal seam; this file drives the real
//! child path — `litany advance` at a dispatched child, whose launcher runs
//! the parent's hop in-process exactly as the detached `litany advance` would.

mod harness;

pub(super) use harness::*;

use super::advance::{RecLauncher, worker_config};
use super::fixtures::*;
use crate::config::Workflow;
use crate::prompt::dispatch::advance::{AdvanceOutcome, run};
use crate::prompt::inbox::{self, inbox_dir, try_acquire};
use crate::prompt::resolve::WorkerConfig;
use std::path::Path;
use tempfile::TempDir;

#[test]
fn a_child_final_response_revives_the_parent_which_delivers_and_steps() {
    // The §2.11 promise end to end, with no scan anywhere: the child's
    // terminal deposit lands in the parent's inbox and the deposit's own
    // probe launches the parent's driver, which delivers the result
    // (transfer + delivery commit) and steps the parent to its own
    // terminal — all from the child's exit, on real git.
    let (_holder, ws, parent, parent_wt, child) = dispatched_child();
    let launcher = RevivingLauncher::new(parent);

    let out = advance_child(&ws, &child, &launcher);
    assert!(matches!(out, AdvanceOutcome::Terminal), "{out:?}");

    // Two launches from the child's exit protocol, in order: itself
    // (pin 1's no-op driver), then the parent it just revived.
    assert_eq!(
        *launcher.invocations.borrow(),
        vec![child.clone(), parent.into()]
    );
    assert_eq!(
        launcher.outcome.borrow().as_deref(),
        Some("Terminal"),
        "the revived parent stepped to its own terminal event"
    );
    // Delivered: the result message is a committed transcript entry on
    // the parent's branch, and its inbox is empty again (§2.11).
    assert!(parent_wt.join(format!("messages/001-{child}.md")).exists());
    assert_eq!(
        std::fs::read_dir(inbox_dir(&ws, parent)).unwrap().count(),
        0
    );
    // Stepped: the parent's own step record landed — the parent reacted
    // without any `litany scan` in the path.
    assert!(
        ws.join(format!("steps/{parent}/001/response.json"))
            .exists()
    );
}

#[test]
fn a_parent_with_a_held_lease_gets_no_second_driver() {
    // §2.11 Writer/driver totality: the probe finds the parent's lease
    // held — its executor is mid-loop and will drain at its next step
    // boundary — so nothing is launched and the result waits in the
    // inbox rather than being delivered by a rival driver.
    let (_holder, ws, parent, _parent_wt, child) = dispatched_child();
    let held = try_acquire(&inbox_dir(&ws, parent)).unwrap().unwrap();
    let launcher = RevivingLauncher::new(parent);

    advance_child(&ws, &child, &launcher);

    assert_eq!(
        *launcher.invocations.borrow(),
        vec![child.clone()],
        "only the self-directed launch: the parent is already driven"
    );
    // The deposit is on disk, undelivered — the running executor's next
    // boundary delivers it (a derived classification, not a flag).
    let pending: Vec<_> = std::fs::read_dir(inbox_dir(&ws, parent))
        .unwrap()
        .flatten()
        .collect();
    assert_eq!(pending.len(), 1);
    let body = std::fs::read_to_string(pending[0].path()).unwrap();
    assert!(body.contains("epitaph: final-response"), "got {body:?}");
    drop(held);
}

/// A child-shaped agent id (§2.3 hyphenated descent) and its parent.
pub(super) const CHILD: &str = "20260101-a1-20260102-b2";
const PARENT: &str = "20260101-a1";

/// A stub-git workspace with `CHILD` materialized on a terminal tail and
/// one pending inbox message, so the hop's drain makes a model call due.
pub(super) fn child_with_mail() -> TempDir {
    let ws = TempDir::new().unwrap();
    let wt = crate::workspace::agent_worktree(ws.path(), CHILD);
    std::fs::create_dir_all(wt.join("messages")).unwrap();
    std::fs::write(wt.join("goal.md"), "the goal").unwrap();
    std::fs::write(wt.join("messages/001-user.md"), "hi").unwrap();
    inbox::deposit(ws.path(), CHILD, PARENT, "go on", &DescentClock).unwrap();
    ws
}

/// The one deposited body in `PARENT`'s inbox.
fn parent_deposit(ws: &Path) -> String {
    let entries: Vec<_> = std::fs::read_dir(inbox_dir(ws, PARENT))
        .unwrap()
        .flatten()
        .collect();
    assert_eq!(entries.len(), 1, "exactly one result deposit");
    std::fs::read_to_string(entries[0].path()).unwrap()
}

#[test]
fn a_stopped_child_deposits_without_reviving_the_parent() {
    // §2.11 pin 2 at the parent: the operator killed this branch, and
    // waking its parent would hand it a stop to undo one level up. The
    // deposit still lands — pre-stop mail waits for the next touch.
    let ws = child_with_mail();
    let (adapter, sleeper, git) = (unreachable_adapter(), StubSleeper::default(), StubGit::ok());
    let (clock, id) = (FixedClock::default(), FixedIdGen);
    let tools = StubToolExecutor::ok();
    let stop = std::sync::atomic::AtomicBool::new(true);
    let rec = RecLauncher::default();
    let mut deps = valid_deps(&adapter, &sleeper, &git, &clock, &id, &tools, ws.path());
    deps.stop = &stop;
    deps.launcher = &rec;

    let out = run(ws.path(), CHILD, None, &deps, &mut || Ok(worker_config())).unwrap();
    assert!(matches!(out, AdvanceOutcome::Terminal));
    assert!(
        parent_deposit(ws.path()).contains("epitaph: stopped"),
        "the stopped result reached the parent's inbox"
    );
    assert!(
        rec.invocations.borrow().is_empty(),
        "neither the branch nor its parent is relaunched"
    );
}

#[test]
fn an_exhausted_child_deposits_without_reviving_the_parent() {
    // §2.11 pin 2 at the parent: the ceiling is derived over the whole
    // tree (§6), so a revived parent would exhaust on its own next check
    // and deposit again — the epitaph-spam cycle, one level up.
    let ws = child_with_mail();
    let (adapter, sleeper, git) = (unreachable_adapter(), StubSleeper::default(), StubGit::ok());
    let (clock, id) = (FixedClock::default(), FixedIdGen);
    let tools = StubToolExecutor::ok();
    let rec = RecLauncher::default();
    let mut deps = valid_deps(&adapter, &sleeper, &git, &clock, &id, &tools, ws.path());
    deps.launcher = &rec;
    let mut capped = || {
        Ok(WorkerConfig {
            workflow: Workflow::parse(
                "events: {}\nbudgets:\n  max_depth: 0\n",
                Path::new("workflow.yaml"),
            )
            .unwrap(),
            ..worker_config()
        })
    };

    let out = run(ws.path(), CHILD, None, &deps, &mut capped).unwrap();
    assert!(matches!(out, AdvanceOutcome::Terminal));
    assert!(
        parent_deposit(ws.path()).contains("epitaph: budget-exhausted"),
        "the exhaustion result reached the parent's inbox"
    );
    assert!(
        rec.invocations.borrow().is_empty(),
        "neither the branch nor its parent is relaunched"
    );
}
