//! Loop-termination cases for the step loop. Split out of
//! [`super::multi_step`] so that file stays under the per-file line cap;
//! the focus here is the `Finish` branch points (a non-`ToolUse` finish
//! terminates without tool work; a tool-executor failure aborts the
//! step).

use super::fixtures::*;
use crate::prompt::{Error, run};
use brazen::FinishReason;
use serde_json::json;

#[test]
fn loop_terminates_on_non_tool_use_finish() {
    // A `Finish{Stop}` terminates without tool work and without a
    // terminal compaction (§2.7 — the stage is deleted). This arm used
    // to be written with `Length`, which since bl-ecf9 is a truncation
    // rather than a termination — see the test below.
    let repo = scaffold_repo(VALID_PER_REPO_PROVIDERS_YAML, Some("body"));
    let harness = scaffold_harness_root();
    let r1 = stream_of(FinishReason::Stop, &[Block::Text("done")]);
    let adapter = StubAdapter::happy(&r1);
    let git = StubGit::ok();
    let (clock, id) = (FixedClock::default(), FixedIdGen);
    let (sleeper, tool_executor) = (StubSleeper::default(), StubToolExecutor::ok());

    run(
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
    .unwrap();
    assert!(tool_executor.invocations.borrow().is_empty());
}

#[test]
fn loop_surfaces_tool_executor_failure_as_tool_exec_error() {
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
    let (sleeper, tool_executor) = (StubSleeper::default(), StubToolExecutor::failing_on("bash"));

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
        Error::ToolExec { tool, .. } => assert_eq!(tool, "bash"),
        other => panic!("expected ToolExec, got {other:?}"),
    }
}

#[test]
fn a_response_cut_at_the_output_cap_is_refused_not_committed() {
    // THE PIN (bl-ecf9, bl-155f): a `Finish{Length}` means the provider
    // stopped at the request's own `max_tokens`, so the last block is
    // cut wherever the counter reached. Committed and executed, that is
    // a `tool_use` with `input: {}` the model then has to reason about,
    // and a conversation the seat calls "came to rest". It is a named
    // failure instead: nothing is committed, no tool is run, and the
    // message names the config key that raises the cap.
    let repo = scaffold_repo(VALID_PER_REPO_PROVIDERS_YAML, Some("body"));
    let harness = scaffold_harness_root();
    let cut = stream_of(
        FinishReason::Length,
        &[Block::ToolUse {
            id: "toolu_01",
            name: "bash",
            input: json!({}),
        }],
    );
    let adapter = StubAdapter::happy(&cut);
    let git = StubGit::ok();
    let (clock, id) = (FixedClock::default(), FixedIdGen);
    let (sleeper, tool_executor) = (StubSleeper::default(), StubToolExecutor::ok());

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
    assert!(matches!(err, Error::OutputTruncated), "{err:?}");
    assert!(
        err.to_string().contains("max_output_tokens"),
        "the decline names the remedy: {err}"
    );
    assert!(
        tool_executor.invocations.borrow().is_empty(),
        "a truncated tool_use is never executed"
    );
    // And it is not retried: the request is a pure function of the tree,
    // so a second attempt is cut at the same byte for a second full cost.
    assert_eq!(
        adapter.observed.borrow().len(),
        2,
        "the version guard and one model call — no retry of a cut stream"
    );
}
