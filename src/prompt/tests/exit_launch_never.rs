//! §2.11 pin 2's negative space at the exit launch: the epitaphs that
//! must never launch a driver — `stopped`, `budget-exhausted` — and the
//! errored executor, which is no terminal event at all. Split from
//! [`super::exit_launch`] (the launching epitaphs and the shared
//! helpers) for the per-file line cap.

use super::exit_launch::{ProbingLauncher, deposit_files, plain_run};
use super::fixtures::*;
use std::io;
use std::sync::atomic::AtomicBool;

#[test]
fn stopped_exit_never_launches() {
    // §2.11 pin 2: `stopped` → never (a relaunch would resurrect the
    // branch the operator just killed) — and never at the parent
    // either: waking it would hand it a stop to undo one level up.
    // The conv-id here is child-shaped (parent `ct`), so a parent-side
    // launch would show up in the recorder.
    let repo = scaffold_repo(VALID_PER_REPO_PROVIDERS_YAML, Some("body"));
    let harness = scaffold_harness_root();
    let adapter = StubAdapter::scripted([StubAdapter::reply_ok(&version_line())]);
    let git = StubGit::ok();
    let (clock, id) = (FixedClock::default(), FixedIdGen);
    let (sleeper, tool_executor) = (StubSleeper::default(), StubToolExecutor::ok());
    let stop = AtomicBool::new(true);
    let launcher = ProbingLauncher::default();

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
    deps.launcher = &launcher;

    plain_run(repo.path(), &deps).unwrap();
    assert!(
        launcher.invocations.borrow().is_empty(),
        "stopped must not launch"
    );
}

#[test]
fn budget_exhausted_exit_never_launches() {
    // §2.11 pin 2: `budget-exhausted` → never (epitaph-spam cycle) —
    // at the parent too, since the ceiling is derived over the whole
    // tree (§6), so a revived parent would exhaust on its own next
    // check and deposit again. The conv-id is child-shaped (parent
    // `ct`): a parent-side launch would be recorded.
    const EXHAUSTING: &str = "events: {}\nbudgets:\n  max_total_tokens: 8\n";
    let repo = scaffold_repo_with_workflow(VALID_PER_REPO_PROVIDERS_YAML, EXHAUSTING, Some("body"));
    let harness = scaffold_harness_root();
    let adapter = StubAdapter::scripted([
        StubAdapter::reply_ok(&version_line()),
        StubAdapter::reply_ok(&stream_of(
            brazen::FinishReason::ToolUse,
            &[Block::ToolUse {
                id: "toolu_01",
                name: "bash",
                input: serde_json::json!({"cmd": "ls"}),
            }],
        )),
        // The step-2 boundary re-resolves before its budget check
        // (bl-e580), so the load-time guard runs once more.
        StubAdapter::reply_ok(&version_line()),
    ]);
    let git = StubGit::ok();
    let (clock, id) = (FixedClock::default(), FixedIdGen);
    let (sleeper, tool_executor) = (StubSleeper::default(), StubToolExecutor::ok());
    let launcher = ProbingLauncher::default();

    let mut deps = valid_deps(
        &adapter,
        &sleeper,
        &git,
        &clock,
        &id,
        &tool_executor,
        harness.path(),
    );
    deps.launcher = &launcher;

    plain_run(repo.path(), &deps).unwrap();
    assert!(
        deposit_files(repo.path())
            .iter()
            .any(|b| b.contains("epitaph: budget-exhausted")),
        "the exhaustion deposit landed"
    );
    assert!(
        launcher.invocations.borrow().is_empty(),
        "exhausted must not launch"
    );
}

#[test]
fn an_errored_executor_never_launches() {
    // An executor error is not a terminal event: it deposits nothing and
    // launches nothing — the accepted crash class (§2.11); the next
    // touch heals.
    let repo = scaffold_repo(VALID_PER_REPO_PROVIDERS_YAML, Some("body"));
    let harness = scaffold_harness_root();
    let adapter = StubAdapter::scripted([
        StubAdapter::reply_ok(&version_line()),
        StubAdapter::reply_err(io::ErrorKind::ConnectionRefused, "no provider"),
    ]);
    let git = StubGit::ok();
    let (clock, id) = (FixedClock::default(), FixedIdGen);
    let (sleeper, tool_executor) = (StubSleeper::default(), StubToolExecutor::ok());
    let launcher = ProbingLauncher::default();

    let mut deps = valid_deps(
        &adapter,
        &sleeper,
        &git,
        &clock,
        &id,
        &tool_executor,
        harness.path(),
    );
    deps.launcher = &launcher;

    plain_run(repo.path(), &deps).unwrap_err();
    assert!(
        launcher.invocations.borrow().is_empty(),
        "an error must not launch"
    );
}
