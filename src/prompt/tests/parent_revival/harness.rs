//! The revival harness: the descent-shaped clock, the launcher that
//! *is* the launched driver, and the two fixtures every beat in
//! [`super`] opens with. Test doubles, kept beside the beats rather
//! than above them.

use super::super::advance::worker_config;
use super::super::fixtures::*;
use crate::prompt::dispatch::advance::{AdvanceOutcome, run};
use crate::prompt::inbox::Launcher;
use crate::prompt::{Clock, Deps, PinnedDocs};
use crate::template::RealGit;
use crate::workspace::agent_name::mint::test_rng;
use std::{cell::RefCell, io, path::Path, path::PathBuf};
use tempfile::TempDir;

/// A hyphen-free compact stamp (§2.3): agent ids stay clean two-token
/// descent segments, so `inbox::parent_of`'s token arithmetic derives
/// the parent the deposit actually landed in.
pub(in crate::prompt::tests) struct DescentClock;
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
pub(super) struct RevivingLauncher {
    pub(super) invocations: RefCell<Vec<String>>,
    target: String,
    pub(super) outcome: RefCell<Option<String>>,
}

impl RevivingLauncher {
    pub(super) fn new(target: &str) -> Self {
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
            data_root: ws,
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
pub(in crate::prompt::tests) fn dispatched_child()
-> (TempDir, PathBuf, &'static str, PathBuf, String) {
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
pub(in crate::prompt::tests) fn advance_child(
    ws: &Path,
    child: &str,
    launcher: &dyn Launcher,
) -> AdvanceOutcome {
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
