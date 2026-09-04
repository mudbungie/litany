//! §6 hop edge arms: the three §2.9 stop check points, the §6 budget
//! boundary, and the error surfaces (acquire failure, resolve failure,
//! missing pinned goal).

use super::advance::{
    AGENT, RecLauncher, model_entry, no_resolve, terminal_tail, worker_config, workspace_with_tail,
};
use super::fixtures::*;
use crate::config::Budgets;
use crate::prompt::dispatch::advance::{AdvanceOutcome, run};
use crate::prompt::inbox::{self, inbox_dir};
use crate::prompt::tool::ToolOutcome;
use crate::prompt::{AdapterRunner, Deps, Error};
use crate::workspace::agent_name::mint::test_rng;
use brazen::{Content, FinishReason};
use std::ffi::OsString;
use std::io;
use std::sync::atomic::{AtomicBool, Ordering};
use tempfile::TempDir;

/// Adapter that sets the stop flag while "in flight" and dies without a
/// terminal `end` — the §2.9 mid-call kill shape.
struct StopMidCallAdapter<'a> {
    flag: &'a AtomicBool,
}
impl AdapterRunner for StopMidCallAdapter<'_> {
    fn run(
        &self,
        _binary: &OsString,
        _args: &[&str],
        _stdin: &[u8],
        _on_line: &mut dyn FnMut(&[u8]) -> io::Result<()>,
    ) -> io::Result<Vec<u8>> {
        self.flag.store(true, Ordering::SeqCst);
        Ok(Vec::new()) // zero lines: a half-stream, no terminal `end`
    }
}

/// Tool executor that sets the stop flag during the tool window.
struct StopMidToolExecutor<'a> {
    flag: &'a AtomicBool,
}
impl crate::prompt::tool::ToolExecutor for StopMidToolExecutor<'_> {
    fn execute(
        &self,
        _call: crate::prompt::tool::ToolCall<'_>,
        _step_dir: &std::path::Path,
        _stop: &AtomicBool,
        _output_bound: Option<crate::config::ToolOutputBound>,
    ) -> Result<ToolOutcome, crate::prompt::ExecError> {
        self.flag.store(true, Ordering::SeqCst);
        Ok(ToolOutcome {
            content: b"interrupted output".to_vec(),
            is_error: false,
        })
    }
}

#[test]
fn a_stop_flag_at_entry_terminates_stopped_without_launching() {
    let (ws, _wt) = workspace_with_tail(&terminal_tail());
    let (clock, id) = (FixedClock::default(), FixedIdGen);
    inbox::deposit(ws.path(), AGENT, "user", "again", &clock).unwrap();
    let (adapter, sleeper, git) = (unreachable_adapter(), StubSleeper::default(), StubGit::ok());
    let tools = StubToolExecutor::ok();
    let rec = RecLauncher::default();
    let stopped = AtomicBool::new(true);
    let mut deps = valid_deps(&adapter, &sleeper, &git, &clock, &id, &tools, ws.path());
    deps.launcher = &rec;
    deps.stop = &stopped;
    let out = run(ws.path(), AGENT, None, &deps, &mut || Ok(worker_config())).unwrap();
    assert!(matches!(out, AdvanceOutcome::Terminal));
    // A stop caught at entry — asserted on the disk record, not a
    // carried payload: the step returned before writing any step record
    // (unlike a final response), and §2.11 pin 2 held — stopped never
    // relaunches, no compactor either (§2.9).
    assert!(!ws.path().join("steps").exists());
    assert!(rec.invocations.borrow().is_empty());
}

#[test]
fn a_stop_during_the_model_call_is_a_stop_not_a_failure() {
    let (ws, wt) = workspace_with_tail(&terminal_tail());
    let (clock, id) = (FixedClock::default(), FixedIdGen);
    inbox::deposit(ws.path(), AGENT, "user", "again", &clock).unwrap();
    let stopped = AtomicBool::new(false);
    let adapter = StopMidCallAdapter { flag: &stopped };
    let (sleeper, git) = (StubSleeper::default(), StubGit::ok());
    let tools = StubToolExecutor::ok();
    let rec = RecLauncher::default();
    let deps = Deps {
        adapter: &adapter,
        sleeper: &sleeper,
        git: &git,
        clock: &clock,
        id_gen: &id,
        tool_executor: &tools,
        config_root: ws.path(),
        data_root: ws.path(),
        adapter_target: None,
        stop: &stopped,
        launcher: &rec,
        rng: test_rng(),
    };
    let out = run(ws.path(), AGENT, None, &deps, &mut || Ok(worker_config())).unwrap();
    assert!(matches!(out, AdvanceOutcome::Terminal));
    // A stop mid-call, not a failure — asserted on the disk record: the
    // branch never advanced to a new committed response (the half-stream
    // was not sealed into the transcript), and no relaunch fired (§2.11
    // pin 2). The stop signature is the absent trailing assistant entry.
    assert!(!wt.join("messages/004-claude-sonnet-5.json").exists());
    assert!(rec.invocations.borrow().is_empty());
}

#[test]
fn a_stop_during_the_tool_window_never_rides_the_baton() {
    let (ws, wt) = workspace_with_tail(&terminal_tail());
    let (clock, id) = (FixedClock::default(), FixedIdGen);
    inbox::deposit(ws.path(), AGENT, "user", "run it", &clock).unwrap();
    let tool_stream = stream_of(
        FinishReason::ToolUse,
        &[Block::ToolUse {
            id: "t1",
            name: "bash",
            input: serde_json::json!({"command": "sleep"}),
        }],
    );
    let adapter = StubAdapter::scripted([StubAdapter::reply_ok(&tool_stream)]);
    let (sleeper, git) = (StubSleeper::default(), StubGit::ok());
    let stopped = AtomicBool::new(false);
    let tools = StopMidToolExecutor { flag: &stopped };
    let rec = RecLauncher::default();
    let deps = Deps {
        adapter: &adapter,
        sleeper: &sleeper,
        git: &git,
        clock: &clock,
        id_gen: &id,
        tool_executor: &tools,
        config_root: ws.path(),
        data_root: ws.path(),
        adapter_target: None,
        stop: &stopped,
        launcher: &rec,
        rng: test_rng(),
    };
    let out = run(ws.path(), AGENT, None, &deps, &mut || Ok(worker_config())).unwrap();
    // Terminal, never ToolsPending: the flag would evaporate across exec,
    // so the hop terminates here (§6). The tool ran and its result
    // committed to the transcript (the disk record), but the stop kept it
    // off the baton — no successor launch fired (§2.11 pin 2).
    assert!(matches!(out, AdvanceOutcome::Terminal));
    assert!(wt.join("messages/005-tool.json").exists());
    assert!(rec.invocations.borrow().is_empty());
}

#[test]
fn budget_exhaustion_at_the_boundary_terminates_without_a_model_call() {
    let (ws, _wt) = workspace_with_tail(&terminal_tail());
    let (clock, id) = (FixedClock::default(), FixedIdGen);
    inbox::deposit(ws.path(), AGENT, "user", "again", &clock).unwrap();
    let (adapter, sleeper, git) = (unreachable_adapter(), StubSleeper::default(), StubGit::ok());
    let tools = StubToolExecutor::ok();
    let rec = RecLauncher::default();
    let mut deps = valid_deps(&adapter, &sleeper, &git, &clock, &id, &tools, ws.path());
    deps.launcher = &rec;
    let mut cfg = worker_config();
    cfg.workflow.budgets = Budgets {
        max_total_tokens: Some(0),
        ..Budgets::default()
    };
    let out = run(ws.path(), AGENT, None, &deps, &mut || Ok(cfg.clone())).unwrap();
    assert!(matches!(out, AdvanceOutcome::Terminal));
    // Budget exhaustion — asserted on the disk record it writes, not a
    // carried payload: the `refs/litany/budget-exhausted/<branch>` marker
    // ref was updated at the boundary before any model call, and §2.11
    // pin 2 held — budget-exhausted never relaunches.
    assert!(git.runs.borrow().iter().any(|(_, args)| {
        args.first().map(String::as_str) == Some("update-ref")
            && args
                .get(1)
                .is_some_and(|r| r.starts_with("refs/litany/budget-exhausted/"))
    }));
    assert!(rec.invocations.borrow().is_empty());
}

#[test]
fn a_resolve_failure_propagates() {
    let (ws, _wt) = workspace_with_tail(&terminal_tail());
    let clock = FixedClock::default();
    inbox::deposit(ws.path(), AGENT, "user", "again", &clock).unwrap();
    let (adapter, sleeper, git) = (unreachable_adapter(), StubSleeper::default(), StubGit::ok());
    let id = FixedIdGen;
    let tools = StubToolExecutor::ok();
    let deps = valid_deps(&adapter, &sleeper, &git, &clock, &id, &tools, ws.path());
    let err = run(ws.path(), AGENT, None, &deps, &mut || {
        Err(Error::RoleMissing("worker".into()))
    })
    .unwrap_err();
    assert!(matches!(err, Error::RoleMissing(_)), "{err}");
}

#[test]
fn a_missing_pinned_goal_surfaces_as_io() {
    let (ws, wt) = workspace_with_tail(&terminal_tail());
    std::fs::remove_file(wt.join("goal.md")).unwrap();
    let (clock, id) = (FixedClock::default(), FixedIdGen);
    inbox::deposit(ws.path(), AGENT, "user", "again", &clock).unwrap();
    let (adapter, sleeper, git) = (unreachable_adapter(), StubSleeper::default(), StubGit::ok());
    let tools = StubToolExecutor::ok();
    let deps = valid_deps(&adapter, &sleeper, &git, &clock, &id, &tools, ws.path());
    let err = run(ws.path(), AGENT, None, &deps, &mut || Ok(worker_config())).unwrap_err();
    assert!(matches!(err, Error::Io(_)), "{err}");
}

#[test]
fn a_broken_inbox_surfaces_as_an_executor_lock_error() {
    let ws = TempDir::new().unwrap();
    std::fs::create_dir_all(ws.path().join("inbox")).unwrap();
    std::fs::write(inbox_dir(ws.path(), AGENT), b"not a dir").unwrap();
    let (adapter, sleeper, git) = (unreachable_adapter(), StubSleeper::default(), StubGit::ok());
    let (clock, id) = (FixedClock::default(), FixedIdGen);
    let tools = StubToolExecutor::ok();
    let deps = valid_deps(&adapter, &sleeper, &git, &clock, &id, &tools, ws.path());
    let err = run(ws.path(), AGENT, None, &deps, &mut no_resolve).unwrap_err();
    assert!(matches!(err, Error::ExecutorLock { .. }), "{err}");
}

/// The pre-settlement debris shape (bl-15f0): a crash-orphaned window
/// that mail already got behind before bl-4187's boundary settlement
/// existed. Appending a settlement could never compose wire-legal
/// (§2.3 pairing is positional), so this form stays the loud decline.
fn buried_unpaired_tail() -> Vec<(&'static str, String)> {
    vec![
        ("001-user.md", "hi".to_string()),
        (
            "002-claude-sonnet-5.json",
            model_entry(&[Content::ToolUse {
                id: "t1".into(),
                name: "bash".into(),
                input: serde_json::json!({"command": "true"}),
                signature: None,
            }]),
        ),
        ("003-user.md", "hello?".to_string()),
    ]
}

#[test]
fn a_buried_unpaired_window_is_declined_loudly_and_never_settled() {
    let (ws, wt) = workspace_with_tail(&buried_unpaired_tail());
    let (adapter, sleeper, git) = (unreachable_adapter(), StubSleeper::default(), StubGit::ok());
    let (clock, id) = (FixedClock::default(), FixedIdGen);
    let tools = StubToolExecutor::ok();
    let deps = valid_deps(&adapter, &sleeper, &git, &clock, &id, &tools, ws.path());
    let err = run(ws.path(), AGENT, None, &deps, &mut no_resolve).unwrap_err();
    assert!(matches!(err, Error::UnpairedToolUse { .. }), "{err}");
    // No settlement entry appeared: the crash settlement refuses the
    // buried form (it could not compose legally), so the transcript is
    // exactly as found.
    assert_eq!(std::fs::read_dir(wt.join("messages")).unwrap().count(), 3);
}

#[test]
fn a_deposit_onto_a_buried_unpaired_window_is_still_declined_after_delivery() {
    // The bl-15f0 end-to-end shape: the hop delivers pending mail
    // *before* deriving warrant, so the tail reads user-side. Before
    // the alternation-wide pairing scan this read as ModelCallDue and
    // the provider rejected the orphan `tool_use` forever; now the hop
    // declines loudly without ever reaching the model.
    let (ws, wt) = workspace_with_tail(&buried_unpaired_tail());
    let clock = FixedClock::default();
    inbox::deposit(ws.path(), AGENT, "user", "again", &clock).unwrap();
    let (adapter, sleeper, git) = (unreachable_adapter(), StubSleeper::default(), StubGit::ok());
    let id = FixedIdGen;
    let tools = StubToolExecutor::ok();
    let deps = valid_deps(&adapter, &sleeper, &git, &clock, &id, &tools, ws.path());
    let err = run(ws.path(), AGENT, None, &deps, &mut no_resolve).unwrap_err();
    assert!(matches!(err, Error::UnpairedToolUse { .. }), "{err}");
    // The mail DID deliver — the decline reads through it, not past it.
    assert!(wt.join("messages/004-user.md").exists());
}
