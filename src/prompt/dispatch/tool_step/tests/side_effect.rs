//! The composed side-effect contract (ARCH §3.3 *Working directory*,
//! §2.3 commit-per-side-effect): what a tool writes lands in the calling
//! agent's worktree and rides the same commit as its `tool_result`
//! entry — asserted over the *production* executor and real git, the
//! only way to see that the pairing actually captures the write (a tool
//! left in the harness's inherited cwd wrote its file outside every
//! worktree, where `git add -A` could never see it, bl-2503).

use super::super::transcript::commit_tool;
use crate::prompt::clock::SystemClock;
use crate::prompt::tool::spawn::PathLookup;
use crate::prompt::tool::{SpawnTool, ToolCall, ToolExecutor};
use crate::template::{GitRunner, RealGit};
use brazen::Content;
use serde_json::json;
use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use tempfile::TempDir;

/// Force the §3.3 second hop to miss so `bash` resolves at the third —
/// the cargo-built `litany` binary as the injected driver target.
struct NoPath;

impl PathLookup for NoPath {
    fn which_on_path(&self, _prefixed_name: &str) -> Option<PathBuf> {
        None
    }
}

#[test]
fn a_bash_write_lands_in_the_worktree_and_rides_the_tool_commit() {
    let agent_id = "agent-2503";
    let ws = TempDir::new().unwrap();
    let worktree = crate::workspace::agent_worktree(ws.path(), agent_id);
    std::fs::create_dir_all(&worktree).unwrap();
    let git = RealGit::new();
    let branch = crate::workspace::agent_ref(agent_id);
    git.run(&worktree, &["init", "-b", &branch]).unwrap();
    git.run(&worktree, &["config", "user.email", "t@t"])
        .unwrap();
    git.run(&worktree, &["config", "user.name", "t"]).unwrap();
    git.run(&worktree, &["config", "core.hooksPath", "/dev/null"])
        .unwrap();
    std::fs::create_dir_all(worktree.join("messages")).unwrap();
    std::fs::write(worktree.join("messages/001-model.json"), b"[]").unwrap();
    git.run(&worktree, &["add", "-A"]).unwrap();
    git.run(&worktree, &["commit", "-m", "dispatch"]).unwrap();

    // Steps sit at the workspace root, outside every worktree (§2.2).
    let step_dir = ws.path().join("steps").join(agent_id).join("001");
    std::fs::create_dir_all(&step_dir).unwrap();

    let empty_data_root = TempDir::new().unwrap();
    let litany = crate::test_support::litany_binary();
    let clock = SystemClock;
    let exec =
        SpawnTool::new(empty_data_root.path(), &clock, &litany).with_path_lookup(Box::new(NoPath));
    let outcome = exec
        .execute(
            ToolCall {
                id: "toolu_write",
                name: "bash",
                input: &json!({ "command": "echo hello > out.txt" }),
            },
            &step_dir,
            &AtomicBool::new(false),
            None,
        )
        .expect("bash executes");
    assert!(!outcome.is_error, "write should succeed: {outcome:?}");

    // Half one: the write landed on the agent's branch, not wherever the
    // test binary was launched from.
    assert_eq!(
        std::fs::read_to_string(worktree.join("out.txt")).unwrap(),
        "hello\n"
    );

    // Half two: the tool commit captures it alongside the result entry.
    let tool_result = Content::ToolResult {
        tool_use_id: "toolu_write".into(),
        content: vec![Content::Text(String::new())],
        is_error: false,
    };
    commit_tool(&worktree, agent_id, &tool_result, &git).unwrap();

    let committed = git
        .run_capture(&worktree, &["show", "--name-only", "--pretty=", "HEAD"])
        .unwrap();
    let names: Vec<&str> = committed.lines().collect();
    assert!(
        names.contains(&"out.txt"),
        "the tool's worktree side effect rides the tool commit: {names:?}"
    );
    assert!(
        names.contains(&"messages/002-tool.json"),
        "the result entry is in the same commit: {names:?}"
    );
    assert!(
        git.run_capture(&worktree, &["status", "--porcelain"])
            .unwrap()
            .is_empty(),
        "nothing is left dirty between sibling tool calls (§3.3)"
    );
}
