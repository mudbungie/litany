//! Full-flow tests for the §6 hop (`dispatch::advance::run`): warrant
//! against constructed on-disk branches, one stepped hop through the
//! stub adapter, the exec-baton `ToolsPending` handoff, and the exit
//! protocol by epitaph value. Real filesystem, stub git/adapter/tools.

use super::exit_launch::PROBE_RETRIES;
use super::fixtures::*;
use crate::config::Workflow;
use crate::prompt::Error;
use crate::prompt::dispatch::advance::{AdvanceOutcome, run};
use crate::prompt::inbox::{self, Launcher, inbox_dir, try_acquire};
use crate::prompt::resolve::WorkerConfig;
use brazen::{Content, FinishReason};
use std::cell::RefCell;
use std::io;
use std::path::{Path, PathBuf};
use tempfile::TempDir;

/// A root-shaped agent id (two hyphen-free tokens, §2.3) so terminal
/// deposits are the structural root no-op (§2.6).
pub(super) const AGENT: &str = "20260101-a1";

/// Recording [`Launcher`] for exit-launch assertions.
#[derive(Default)]
pub(super) struct RecLauncher {
    pub(super) invocations: RefCell<Vec<String>>,
}
impl Launcher for RecLauncher {
    fn launch(&self, _ws: &Path, agent: &str) -> io::Result<()> {
        self.invocations.borrow_mut().push(agent.to_string());
        Ok(())
    }
}

/// An injected-resolve [`WorkerConfig`] matching the stub adapter's
/// fixture model. No version guard runs (resolution is injected).
pub(super) fn worker_config() -> WorkerConfig {
    WorkerConfig {
        role: "worker".into(),
        model_id: "claude-sonnet-5".into(),
        provider_row: "anthropic".into(),
        effort: None,
        priority: None,
        // The grant the fixtures' model output calls against (§4.3): a
        // tool a role does not grant is declined at execution, never run
        // (`dispatch/tool_step.rs::refusal`).
        tools: vec!["bash".into()],
        config_commit: super::stubs::STUB_SHA.into(),
        soul: "be helpful".into(),
        binary: "bz".into(),
        workflow: Workflow::parse("events: {}\n", std::path::Path::new("workflow.yaml")).unwrap(),
        workflow_commit: super::stubs::STUB_SHA.into(),
        manifest: None,
        expect_handshake: false,
    }
}

/// A workspace with a materialized agent worktree: `goal.md` plus the
/// given transcript entries (name, body) under `messages/`.
pub(super) fn workspace_with_tail(entries: &[(&str, String)]) -> (TempDir, PathBuf) {
    let ws = TempDir::new().unwrap();
    let wt = crate::workspace::agent_worktree(ws.path(), AGENT);
    std::fs::create_dir_all(wt.join("messages")).unwrap();
    std::fs::write(wt.join("goal.md"), "the goal").unwrap();
    for (name, body) in entries {
        std::fs::write(wt.join("messages").join(name), body).unwrap();
    }
    (ws, wt)
}

/// A canonical model-output entry body (§2.3): a `Content` array.
pub(super) fn model_entry(blocks: &[Content]) -> String {
    serde_json::to_string(blocks).unwrap()
}

/// A terminal tail: user message answered by a final text response.
pub(super) fn terminal_tail() -> Vec<(&'static str, String)> {
    vec![
        ("001-user.md", "hi".to_string()),
        (
            "002-claude-sonnet-5.json",
            model_entry(&[Content::Text("final".into())]),
        ),
    ]
}

/// A resolve that must never be consulted (no-op hops resolve nothing,
/// §6): a plain fn, passable as `&mut no_resolve`.
pub(super) fn no_resolve() -> Result<WorkerConfig, Error> {
    panic!("resolve must not run on a no-op hop")
}

#[test]
#[should_panic(expected = "resolve must not run")]
fn no_resolve_is_a_tripwire() {
    let _ = no_resolve();
}

/// Probe until the lease frees, retrying across the fork→exec
/// fd-inheritance window on the shared [`PROBE_RETRIES`] budget (the
/// established pattern — a parallel test's spawned child can briefly
/// hold an inherited copy of a dropped fd; a real contender holds for
/// the whole window).
pub(super) fn eventually_free(ws: &Path, agent: &str) -> bool {
    free_within(ws, agent, PROBE_RETRIES)
}

/// [`eventually_free`] with the retry budget injected, so the
/// retry-and-give-up arms are directly exercisable.
///
/// The budget is a count of attempts, never a wall-clock deadline. A
/// deadline expires on machine load rather than on evidence: with
/// several agents measuring coverage at once, the first probe can
/// return *after* a short deadline has already passed, so the retry
/// sleep below never runs and the 100% floor reports one uncovered
/// line on a diff that touched nothing (the bl-1c2e flake). A count
/// makes both arms structural — a lease held for the whole budget
/// sleeps `retries - 1` times whatever else the machine is doing.
pub(super) fn free_within(ws: &Path, agent: &str, retries: u32) -> bool {
    for attempt in 0..retries {
        if attempt > 0 {
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        if try_acquire(&inbox_dir(ws, agent)).unwrap().is_some() {
            return true;
        }
    }
    false
}

#[test]
fn free_within_gives_up_on_a_genuinely_held_lease() {
    let ws = TempDir::new().unwrap();
    let _held = try_acquire(&inbox_dir(ws.path(), AGENT)).unwrap().unwrap();
    assert!(!free_within(ws.path(), AGENT, 2));
}

#[test]
fn already_driven_is_a_clean_noop_without_resolving() {
    let (ws, _wt) = workspace_with_tail(&terminal_tail());
    let _held = try_acquire(&inbox_dir(ws.path(), AGENT)).unwrap().unwrap();
    let (adapter, sleeper, git) = (unreachable_adapter(), StubSleeper::default(), StubGit::ok());
    let (clock, id) = (FixedClock::default(), FixedIdGen);
    let tools = StubToolExecutor::ok();
    let deps = valid_deps(&adapter, &sleeper, &git, &clock, &id, &tools, ws.path());
    let out = run(ws.path(), AGENT, None, &deps, &mut no_resolve).unwrap();
    assert!(matches!(out, AdvanceOutcome::AlreadyDriven));
}

#[test]
fn empty_workspace_is_nothing_to_do() {
    // No worktree, no inbox: quiescent-torn-down, nothing due.
    let ws = TempDir::new().unwrap();
    let (adapter, sleeper, git) = (unreachable_adapter(), StubSleeper::default(), StubGit::ok());
    let (clock, id) = (FixedClock::default(), FixedIdGen);
    let tools = StubToolExecutor::ok();
    let deps = valid_deps(&adapter, &sleeper, &git, &clock, &id, &tools, ws.path());
    let out = run(ws.path(), AGENT, None, &deps, &mut no_resolve).unwrap();
    assert!(matches!(out, AdvanceOutcome::NothingToDo));
}

#[test]
fn terminal_tail_with_empty_inbox_is_the_pin_1_silent_exit() {
    let (ws, wt) = workspace_with_tail(&terminal_tail());
    let (adapter, sleeper, git) = (unreachable_adapter(), StubSleeper::default(), StubGit::ok());
    let (clock, id) = (FixedClock::default(), FixedIdGen);
    let tools = StubToolExecutor::ok();
    let deps = valid_deps(&adapter, &sleeper, &git, &clock, &id, &tools, ws.path());
    let out = run(ws.path(), AGENT, None, &deps, &mut no_resolve).unwrap();
    assert!(matches!(out, AdvanceOutcome::NothingToDo));
    // No step: no step records appeared, the transcript is untouched.
    assert!(!ws.path().join("steps").exists());
    assert_eq!(std::fs::read_dir(wt.join("messages")).unwrap().count(), 2);
}

#[test]
fn a_deposit_steps_the_branch_to_a_new_final_response() {
    // The reprompt chain (§2.4/§6): deliver → warrant → one step →
    // terminal → exit protocol (deposit no-op for a root, release,
    // final-response exit launch).
    let (ws, wt) = workspace_with_tail(&terminal_tail());
    let (clock, id) = (FixedClock::default(), FixedIdGen);
    inbox::deposit(ws.path(), AGENT, "user", "again", &clock).unwrap();
    let adapter = StubAdapter::scripted([StubAdapter::reply_ok(&happy_response_bytes())]);
    let (sleeper, git) = (StubSleeper::default(), StubGit::ok());
    let tools = StubToolExecutor::ok();
    let rec = RecLauncher::default();
    let mut deps = valid_deps(&adapter, &sleeper, &git, &clock, &id, &tools, ws.path());
    deps.launcher = &rec;
    let out = run(ws.path(), AGENT, None, &deps, &mut || Ok(worker_config())).unwrap();
    assert!(matches!(out, AdvanceOutcome::Terminal));
    // Delivered at the boundary, then answered by the new step.
    let delivered = std::fs::read_to_string(wt.join("messages/003-user.md")).unwrap();
    assert!(delivered.contains("again"), "got {delivered:?}");
    assert!(wt.join("messages/004-claude-sonnet-5.json").exists());
    // The step record landed at the derived sequence (steps/ was empty).
    assert!(
        ws.path()
            .join(format!("steps/{AGENT}/001/response.json"))
            .exists()
    );
    // The terminal event is a final response — asserted on the disk
    // record, not a carried payload: the exit protocol's self-launch
    // fired (§2.11 pin 2 — only a final response relaunches), and no
    // terminal compaction ran (§2.7 — the stage is deleted).
    assert_eq!(*rec.invocations.borrow(), vec![AGENT.to_string()]);
    // The lease was released before the launch: reacquirable again.
    assert!(eventually_free(ws.path(), AGENT));
}

#[test]
fn a_stop_felling_a_tool_mid_window_is_the_stopped_terminal() {
    // §2.9 step 3 through the hop: the group SIGTERM fells the running
    // tool with the stop flag set — the window reports the stop and the
    // hop concludes the stopped terminal, never a fault.
    let (ws, _wt) = workspace_with_tail(&terminal_tail());
    let (clock, id) = (FixedClock::default(), FixedIdGen);
    inbox::deposit(ws.path(), AGENT, "user", "run it", &clock).unwrap();
    let tool_stream = stream_of(
        FinishReason::ToolUse,
        &[Block::ToolUse {
            id: "t1",
            name: "bash",
            input: serde_json::json!({"command": "true"}),
        }],
    );
    let adapter = StubAdapter::scripted([StubAdapter::reply_ok(&tool_stream)]);
    let (sleeper, git) = (StubSleeper::default(), StubGit::ok());
    let tools = StubToolExecutor::stop_killed_on("bash");
    let stop = std::sync::atomic::AtomicBool::new(false);
    let mut deps = valid_deps(&adapter, &sleeper, &git, &clock, &id, &tools, ws.path());
    deps.stop = &stop;
    let out = run(ws.path(), AGENT, None, &deps, &mut || Ok(worker_config())).unwrap();
    assert!(matches!(out, AdvanceOutcome::Terminal), "got {out:?}");
    assert!(eventually_free(ws.path(), AGENT));
}

#[test]
fn a_tool_use_step_hands_off_as_tools_pending_with_the_lease_held() {
    let (ws, wt) = workspace_with_tail(&terminal_tail());
    let (clock, id) = (FixedClock::default(), FixedIdGen);
    inbox::deposit(ws.path(), AGENT, "user", "run it", &clock).unwrap();
    let tool_stream = stream_of(
        FinishReason::ToolUse,
        &[Block::ToolUse {
            id: "t1",
            name: "bash",
            input: serde_json::json!({"command": "true"}),
        }],
    );
    let adapter = StubAdapter::scripted([StubAdapter::reply_ok(&tool_stream)]);
    let (sleeper, git) = (StubSleeper::default(), StubGit::ok());
    let tools = StubToolExecutor::ok();
    let rec = RecLauncher::default();
    let mut deps = valid_deps(&adapter, &sleeper, &git, &clock, &id, &tools, ws.path());
    deps.launcher = &rec;
    let out = run(ws.path(), AGENT, None, &deps, &mut || Ok(worker_config())).unwrap();
    let AdvanceOutcome::ToolsPending(lease) = out else {
        panic!("expected ToolsPending");
    };
    // The tool ran and its result committed (§2.3): the successor's
    // warrant will find the tail user-side.
    assert_eq!(tools.invocations.borrow().len(), 1);
    assert!(wt.join("messages/005-tool.json").exists());
    // The lease rides the baton: held while the outcome lives.
    assert!(try_acquire(&inbox_dir(ws.path(), AGENT)).unwrap().is_none());
    drop(lease);
    assert!(eventually_free(ws.path(), AGENT));
    // No exit protocol on a handoff: no launch.
    assert!(rec.invocations.borrow().is_empty());
}
