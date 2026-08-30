//! The stopped exit settles its own tool window (ARCH §2.9 step 3,
//! [`super::super::settle`]): a `litany stop` landing mid-window leaves
//! a tail an ordinary deposit can revive, never the §6 unpaired
//! decline.

use super::multi::{Fixture, Scripted};
use super::{Resolution, branch_with_step};
use crate::prompt::dispatch::tool_step::{ToolWindow, run_tool_calls};
use brazen::{Content, Role};
use serde_json::json;
use std::sync::atomic::AtomicBool;
use tempfile::TempDir;

fn tool_use(id: &str, name: &str) -> Content {
    Content::ToolUse {
        id: id.into(),
        name: name.into(),
        input: json!({}),
        signature: None,
    }
}

/// The entry's blocks, read back from the committed transcript file.
fn entry(worktree: &std::path::Path, file: &str) -> Vec<Content> {
    let bytes = std::fs::read(worktree.join("messages").join(file)).expect("entry committed");
    serde_json::from_slice(&bytes).expect("entry is canonical blocks")
}

#[test]
fn a_stop_mid_window_settles_every_unanswered_block() {
    // One window, four blocks: prose (no invocation), `alpha` (ran and
    // committed before the stop), `boom` (felled by the group SIGTERM
    // with the stop flag set — §2.9 steps 1-2), `gamma` (never entered).
    // The exit commits an in-band `is_error` result for `boom` and
    // `gamma`, and leaves `alpha`'s single real result alone.
    let agent_id = "agent-b98d";
    let ws = TempDir::new().unwrap();
    let fx = Fixture::new();
    let (worktree, step_dir_rel) = branch_with_step(&ws, agent_id, &fx.git);
    let exec = Scripted {
        kill: &["boom"],
        ..Scripted::new()
    };
    let stop = AtomicBool::new(true);
    let deps = fx.deps(&exec, &stop);
    let content = vec![
        Content::Text("working on it".into()),
        tool_use("t1", "alpha"),
        tool_use("t2", "boom"),
        tool_use("t3", "gamma"),
    ];
    let grant = ["alpha".to_string(), "boom".to_string(), "gamma".to_string()];
    let resolution = Resolution::new();
    let window = run_tool_calls(
        ws.path(),
        &worktree,
        agent_id,
        &resolution.of(crate::prompt::WORKER_ROLE, &grant),
        &step_dir_rel,
        &content,
        &deps,
    )
    .unwrap();
    assert!(matches!(window, ToolWindow::Stopped), "the stop ceases");
    assert_eq!(exec.log.borrow().len(), 2, "`gamma` was never entered");

    // `alpha`'s own result stands, unduplicated and not re-marked.
    let alpha = entry(&worktree, "002-tool.json");
    let [
        Content::ToolResult {
            tool_use_id,
            is_error,
            ..
        },
    ] = alpha.as_slice()
    else {
        panic!("one result block: {alpha:?}");
    };
    assert_eq!(tool_use_id, "t1");
    assert!(!is_error);

    // Each unanswered id — the felled one and the unreached one — gets
    // exactly one in-band settlement, in emission order.
    for (file, id) in [("003-tool.json", "t2"), ("004-tool.json", "t3")] {
        let blocks = entry(&worktree, file);
        let [
            Content::ToolResult {
                tool_use_id,
                content,
                is_error,
            },
        ] = blocks.as_slice()
        else {
            panic!("one result block: {blocks:?}");
        };
        assert_eq!(tool_use_id, id);
        assert!(is_error, "the settlement is in-band `is_error`");
        let [Content::Text(text)] = content.as_slice() else {
            panic!("one text block: {content:?}");
        };
        assert!(text.contains("did not return"), "{text}");
        assert!(text.contains("§2.9"), "it names the stop: {text}");
    }
    assert!(
        !worktree.join("messages/005-tool.json").exists(),
        "the prose block is not an invocation and settles nothing"
    );

    // The point of settling: the tail is now tool-side, which §6 reads
    // as `ModelCallDue` — so a deposit revives this agent by the
    // ordinary path instead of meeting the unpaired decline.
    let tail = crate::prompt::dispatch::assembler::transcript(&worktree).unwrap();
    assert_eq!(tail.last().unwrap().role, Role::Tool);
}
