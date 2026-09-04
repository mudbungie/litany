//! §2.9 step-3 stop path: the executor catches SIGTERM, deposits the
//! branch's result with a `stopped` epitaph on its way out, and exits
//! without compacting — the deposit is executor-side ("Return is not a
//! verb"). Two check points are exercised: a stop seen *between* steps
//! (before the next model call) and a stop delivered *during* the model
//! call (which kills `bz`, leaving `response.json` without a trailing
//! `end` — the on-disk stop signature, which this path must not disturb).
//!
//! The `FixedClock` compact stamp is `ct-1`, so the conv-id
//! `ct-1-deadbeef` parses as a child of `ct` (§2.11 token arithmetic) —
//! the deposit therefore lands a real file under `inbox/ct/` rather than
//! no-opping, letting these tests assert the `stopped` epitaph directly.

mod mid_call;

use super::fixtures::*;
use crate::prompt::adapter::AdapterRunner;
use crate::prompt::step::step_dir_rel;
use crate::prompt::{Deps, run};
use crate::workspace::agent_name::mint::test_rng;
use brazen::FinishReason;
use serde_json::json;
use std::ffi::OsString;
use std::io;
use std::sync::atomic::{AtomicBool, Ordering};

/// Read the single deposited result message under `inbox/ct/`.
fn deposited_result(repo: &std::path::Path) -> String {
    let dir = repo.join("inbox").join("ct");
    let entry = std::fs::read_dir(&dir)
        .unwrap_or_else(|e| panic!("no inbox/ct dir ({e})"))
        .map(|e| e.unwrap().path())
        .find(|p| p.extension().is_some_and(|x| x == "md"))
        .expect("a deposited .md result");
    std::fs::read_to_string(entry).unwrap()
}

#[test]
fn stop_between_steps_deposits_stopped_and_skips_compaction() {
    // The stop flag is already set when the loop reaches its first check
    // point (after the step-1 dispatch commit, before the model call): the
    // loop breaks straight to the on-the-way-out deposit. The adapter is
    // scripted with the version guard reply only — a model call would
    // panic "called more times than scripted", proving none fired.
    let repo = scaffold_repo(VALID_PER_REPO_PROVIDERS_YAML, Some("body"));
    let harness = scaffold_harness_root();
    let adapter = StubAdapter::scripted([StubAdapter::reply_ok(&version_line())]);
    let git = StubGit::ok();
    let (clock, id) = (FixedClock::default(), FixedIdGen);
    let (sleeper, tool_executor) = (StubSleeper::default(), StubToolExecutor::ok());
    let stop = AtomicBool::new(true);

    let mut deps = valid_deps(
        &adapter,
        &sleeper,
        &git,
        &clock,
        &id,
        &tool_executor,
        harness.path(),
    );
    deps.stop = &stop;

    let branch = run(
        repo.path(),
        "hi",
        None,
        None,
        None,
        crate::prompt::PinnedDocs::none(),
        None,
        &deps,
    )
    .unwrap();
    assert_eq!(branch, "ct-1-deadbeef");
    // No terminal compactor for a stopped branch (§2.9, like exhausted).
    // The result deposited on the way out carries the `stopped` epitaph.
    assert!(deposited_result(repo.path()).contains("epitaph: stopped"));
    // The model call never ran, so no step-1 response record exists.
    assert!(!repo.path().join(step_dir_rel("ct-1-deadbeef", 1)).exists());
}

/// Adapter whose model call simulates SIGTERM arriving mid-stream: it
/// flips the injected stop flag, then dies after a lone `message_start`
/// with no trailing `end` — exactly `bz`'s §2.9 on-disk signature. The
/// version-guard call is served normally and does not flip the flag.
struct FlipMidCall<'a> {
    flag: &'a AtomicBool,
}

impl AdapterRunner for FlipMidCall<'_> {
    fn run(
        &self,
        _binary: &OsString,
        args: &[&str],
        _stdin: &[u8],
        on_line: &mut dyn FnMut(&[u8]) -> io::Result<()>,
    ) -> io::Result<Vec<u8>> {
        if args.contains(&"--version") {
            for line in version_line().split(|b| *b == b'\n') {
                if !line.is_empty() {
                    on_line(line)?;
                }
            }
            return Ok(Vec::new());
        }
        // The model call: SIGTERM lands now. Emit a `message_start` and
        // stop — no `end`, the stop signature — then set the flag so the
        // executor's next check point reads the stop.
        on_line(br#"{"type":"message_start","v":1,"role":"assistant"}"#)?;
        self.flag.store(true, Ordering::SeqCst);
        Ok(Vec::new())
    }
}

#[test]
fn stop_during_model_call_deposits_stopped_and_preserves_missing_end() {
    let repo = scaffold_repo(VALID_PER_REPO_PROVIDERS_YAML, Some("body"));
    let harness = scaffold_harness_root();
    let stop = AtomicBool::new(false);
    let adapter = FlipMidCall { flag: &stop };
    let git = StubGit::ok();
    let (clock, id) = (FixedClock::default(), FixedIdGen);
    let (sleeper, tool_executor) = (StubSleeper::default(), StubToolExecutor::ok());

    let deps = Deps {
        adapter: &adapter,
        sleeper: &sleeper,
        git: &git,
        clock: &clock,
        id_gen: &id,
        tool_executor: &tool_executor,
        config_root: harness.path(),
        data_root: harness.path(),
        adapter_target: None,
        stop: &stop,
        launcher: no_launch(),
        rng: test_rng(),
    };

    // The half-stream mid-call would surface as `AdapterHalfStream`, but
    // with the stop flag set the executor treats it as a stop: `run`
    // returns Ok, not an error.
    let branch = run(
        repo.path(),
        "hi",
        None,
        None,
        None,
        crate::prompt::PinnedDocs::none(),
        None,
        &deps,
    )
    .unwrap();
    assert_eq!(branch, "ct-1-deadbeef");
    assert!(deposited_result(repo.path()).contains("epitaph: stopped"));

    // The stop signature is intact: `response.json` was written (the
    // model call ran) and closed without a trailing `end` event. The
    // deposit above wrote to a different tree (`inbox/`), untouched here.
    let response = repo
        .path()
        .join(step_dir_rel("ct-1-deadbeef", 1))
        .join("response.json");
    let lines = parse_jsonl(&std::fs::read(&response).unwrap());
    assert_eq!(lines.first().unwrap()["type"], "message_start");
    assert!(
        !lines.iter().any(|e| e["type"] == "end"),
        "stopped step must have no terminal `end` (the §2.9 signature)"
    );
}

/// One `tool_use` step, and the tool is cut down by the executor's own
/// group SIGTERM (§2.9 steps 1-2): the stub flips the stop flag (the
/// handler ran) and returns `KilledBySignal`. That is the stop, not a
/// fault — `run` returns Ok with a `stopped` deposit and no compaction,
/// the *same* terminal sequence as a stop landing in the model-call
/// window. Before this wiring the `KilledBySignal` propagated as
/// `ToolExec` and the harness exited non-zero (the crash shape).
#[test]
fn stop_during_tool_execution_deposits_stopped_and_skips_compaction() {
    let repo = scaffold_repo(VALID_PER_REPO_PROVIDERS_YAML, Some("body"));
    let harness = scaffold_harness_root();
    let r1 = stream_of(
        FinishReason::ToolUse,
        &[Block::ToolUse {
            id: "toolu_01",
            name: "bash",
            input: json!({"cmd": "ls"}),
        }],
    );
    let adapter = StubAdapter::happy(&r1);
    let git = StubGit::ok();
    let (clock, id) = (FixedClock::default(), FixedIdGen);
    let stop = AtomicBool::new(false);
    let (sleeper, tool_executor) = (
        StubSleeper::default(),
        StubToolExecutor::stop_killed_on("bash"),
    );

    let mut deps = valid_deps(
        &adapter,
        &sleeper,
        &git,
        &clock,
        &id,
        &tool_executor,
        harness.path(),
    );
    deps.stop = &stop;

    let branch = run(
        repo.path(),
        "hi",
        None,
        None,
        None,
        crate::prompt::PinnedDocs::none(),
        None,
        &deps,
    )
    .unwrap();
    assert_eq!(branch, "ct-1-deadbeef");
    // The tool was entered before the stop felled it.
    assert_eq!(tool_executor.invocations.borrow().len(), 1);
    // No terminal compactor for a stopped branch (§2.9), and the result
    // deposited on the way out carries the `stopped` epitaph.
    assert!(deposited_result(repo.path()).contains("epitaph: stopped"));
    // The exit settled its own window first (§2.9 step 3): the felled
    // invocation is answered in band, so the branch tip is a *paired*
    // tail — `litany advance` reads `ModelCallDue`, not the §6 unpaired
    // decline, and a deposit revives this agent by the ordinary path.
    let worktree = crate::workspace::agent_worktree(repo.path(), "ct-1-deadbeef");
    let settled = std::fs::read_to_string(worktree.join("messages/003-tool.json")).unwrap();
    assert!(settled.contains("toolu_01"), "{settled}");
    assert!(settled.contains("\"is_error\":true"), "{settled}");
    assert!(settled.contains("did not return"), "{settled}");
}

/// A tool killed by a signal with *no* stop pending is a genuine crash
/// (§2.10), not a stop: the loop surfaces it as `ToolExec`, never the
/// stopped-deposit exit — the out-of-stop-path classification.
#[test]
fn tool_killed_without_stop_surfaces_as_tool_exec_error() {
    let repo = scaffold_repo(VALID_PER_REPO_PROVIDERS_YAML, Some("body"));
    let harness = scaffold_harness_root();
    let r1 = stream_of(
        FinishReason::ToolUse,
        &[Block::ToolUse {
            id: "toolu_01",
            name: "bash",
            input: json!({"cmd": "ls"}),
        }],
    );
    let adapter = StubAdapter::happy(&r1);
    let git = StubGit::ok();
    let (clock, id) = (FixedClock::default(), FixedIdGen);
    let (sleeper, tool_executor) = (StubSleeper::default(), StubToolExecutor::killed_on("bash"));

    let err = run(
        repo.path(),
        "hi",
        None,
        None,
        None,
        crate::prompt::PinnedDocs::none(),
        None,
        &valid_deps(
            &adapter,
            &sleeper,
            &git,
            &clock,
            &id,
            &tool_executor,
            harness.path(),
        ),
    )
    .unwrap_err();
    match err {
        crate::prompt::Error::ToolExec { tool, .. } => assert_eq!(tool, "bash"),
        other => panic!("expected ToolExec, got {other:?}"),
    }
}
