//! One step's tool window runs every `tool_use` block it carries, in
//! emission order (ARCH §2.5 pairing, §2.3 sequence). Split from
//! `multi_step.rs` for the 300-line repo cap.

use super::super::fixtures::*;
use super::final_stream;
use crate::prompt::run;
use brazen::FinishReason;
use serde_json::{Value, json};

#[test]
fn loop_runs_each_tool_use_block_in_one_step_in_emission_order() {
    let repo = scaffold_repo(VALID_PER_REPO_PROVIDERS_YAML, Some("body"));
    let harness = scaffold_harness_root();
    let two_tool_use = stream_of(
        FinishReason::ToolUse,
        &[
            Block::ToolUse {
                id: "toolu_a",
                name: "bash",
                input: json!({"cmd": "ls"}),
            },
            Block::ToolUse {
                id: "toolu_b",
                name: "read_file",
                input: json!({"path": "/x"}),
            },
        ],
    );
    let adapter = StubAdapter::scripted([
        StubAdapter::reply_ok(&version_line()),
        StubAdapter::reply_ok(&two_tool_use),
        // Step 2 re-resolves at its boundary (bl-e580) — its own guard.
        StubAdapter::reply_ok(&version_line()),
        StubAdapter::reply_ok(&final_stream()),
    ]);
    let git = StubGit::ok();
    let (clock, id) = (FixedClock::default(), FixedIdGen);
    let (sleeper, tool_executor) = (StubSleeper::default(), StubToolExecutor::ok());

    run(
        repo.path(),
        "do two things",
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

    let invocations = tool_executor.invocations.borrow().clone();
    assert_eq!(invocations.len(), 2);
    let pair = |c: &(_, String, String, _)| (c.1.clone(), c.2.clone());
    assert_eq!(pair(&invocations[0]), ("toolu_a".into(), "bash".into()));
    assert_eq!(
        pair(&invocations[1]),
        ("toolu_b".into(), "read_file".into())
    );

    let req2: Value = serde_json::from_slice(
        &std::fs::read(repo.path().join("steps/ct-1-deadbeef/002/request.json")).unwrap(),
    )
    .unwrap();
    let user_blocks = req2["messages"][2]["content"].as_array().unwrap();
    assert_eq!(user_blocks.len(), 2);
    assert_eq!(user_blocks[0]["tool_use_id"], "toolu_a");
    assert_eq!(user_blocks[1]["tool_use_id"], "toolu_b");
}
