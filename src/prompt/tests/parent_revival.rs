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

use super::advance::{RecLauncher, worker_config};
use super::fixtures::*;
use crate::config::Workflow;
use crate::prompt::dispatch::advance::{AdvanceOutcome, run};
use crate::prompt::inbox::{self, Launcher, inbox_dir, try_acquire};
use crate::prompt::resolve::WorkerConfig;
use crate::prompt::{Clock, Deps, PinnedDocs};
use crate::template::RealGit;
use crate::workspace::agent_name::mint::test_rng;
use std::{cell::RefCell, io, path::Path, path::PathBuf};
use tempfile::TempDir;

/// A hyphen-free compact stamp (§2.3): agent ids stay clean two-token
/// descent segments, so `inbox::parent_of`'s token arithmetic derives
/// the parent the deposit actually landed in.
pub(super) struct DescentClock;
impl Clock for DescentClock {
    fn now_iso8601(&self) -> String {
        "iso".into()
    }
    fn now_compact(&self) -> String {
        "ct1".into()
    }
}

/// A launcher that *is* the launched driver: it records every launch
/// and, the first time it is asked to launch the agent under test, runs
/// that agent's `litany advance` hop in-process — what the detached
/// spawn does, minus the process. That nested hop is handed the inert
/// launcher, so its own exit protocol stops the in-process recursion
/// (the real chain terminates on pin 1's no-op driver instead).
struct RevivingLauncher {
    invocations: RefCell<Vec<String>>,
    target: String,
    outcome: RefCell<Option<String>>,
}

impl RevivingLauncher {
    fn new(target: &str) -> Self {
        Self {
            invocations: RefCell::new(Vec::new()),
            target: target.to_string(),
            outcome: RefCell::new(None),
        }
    }
}

impl Launcher for RevivingLauncher {
    fn launch(&self, ws: &Path, agent: &str) -> io::Result<()> {
        self.invocations.borrow_mut().push(agent.to_string());
        if agent != self.target || self.outcome.borrow().is_some() {
            return Ok(());
        }
        let adapter = StubAdapter::scripted([StubAdapter::reply_ok(&happy_response_bytes())]);
        let (sleeper, tools, stub_git) = (
            StubSleeper::default(),
            StubToolExecutor::ok(),
            StubGit::ok(),
        );
        let (clock, id) = (FixedClock::default(), FixedIdGen);
        let git = RealGit::new();
        let mut deps = Deps {
            adapter: &adapter,
            sleeper: &sleeper,
            git: &stub_git,
            clock: &clock,
            id_gen: &id,
            tool_executor: &tools,
            config_root: ws,
            adapter_target: None,
            stop: never_stopped(),
            launcher: no_launch(),
            rng: test_rng(),
        };
        deps.git = &git;
        let out = run(ws, agent, None, &deps, &mut || Ok(worker_config()))
            .map_err(|e| io::Error::other(e.to_string()))?;
        *self.outcome.borrow_mut() = Some(format!("{out:?}"));
        Ok(())
    }
}

/// A real workspace with a root parent and a dispatched worker child,
/// the child's inbox holding its dispatch message (§2.5 front door).
/// Returns `(holder, workspace, parent id, parent worktree, child id)`.
pub(super) fn dispatched_child() -> (TempDir, PathBuf, &'static str, PathBuf, String) {
    use crate::prompt::child_dispatch::{ChildDispatchRequest, run as dispatch_child};
    use crate::workspace::fixture;

    let (holder, ws) = fixture::workspace();
    let parent = "20260101-a1";
    let parent_wt = fixture::spawn_root(&ws, parent);
    let child = dispatch_child(
        &ChildDispatchRequest {
            repo: &ws,
            parent_branch: parent,
            parent_worktree: &parent_wt,
            role: "worker",
            goal: "do it",
            name: None,
            fork_point: None,
            cwd: None,
            pins: PinnedDocs::none(),
        },
        &RealGit::new(),
        &DescentClock,
        &FixedIdGen,
        no_launch(),
        test_rng(),
    )
    .unwrap();
    (holder, ws, parent, parent_wt, child)
}

/// Advance the child to its terminal event under `launcher` — one hop:
/// deliver the dispatch message, step, final response, exit protocol.
pub(super) fn advance_child(ws: &Path, child: &str, launcher: &dyn Launcher) -> AdvanceOutcome {
    let adapter = StubAdapter::scripted([StubAdapter::reply_ok(&happy_response_bytes())]);
    let (sleeper, tools, stub_git) = (
        StubSleeper::default(),
        StubToolExecutor::ok(),
        StubGit::ok(),
    );
    let (clock, id) = (FixedClock::default(), FixedIdGen);
    let git = RealGit::new();
    let mut deps = valid_deps(&adapter, &sleeper, &stub_git, &clock, &id, &tools, ws);
    deps.git = &git;
    deps.launcher = launcher;
    run(ws, child, None, &deps, &mut || Ok(worker_config())).unwrap()
}

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
