//! The bl-4231 defect at the fork, over real git: a child dispatched
//! from inside its parent's tool step inherits the parent's *unsettled*
//! tail, and its first model-facing history must still be one the wire
//! accepts (ARCH §2.5 pairing, §2.3 *Fork and inheritance*).
//!
//! The dispatched role is the **compactor** — since bl-5a36 the one
//! child whose tree retains the inherited transcript at all
//! ([`super::super::super::inherited`]); every other child's dialog
//! prune removes the tail wholesale, transcript and all.
//!
//! The same predicate is asserted twice — unanswered on the parent's tip
//! (the fork point genuinely carries the defect shape) and empty on the
//! child's tree (the dispatch commit made its record honest).

use super::super::MESSAGES_DIR;
use super::{tool_result, tool_use};
use crate::prompt::child_dispatch::{ChildDispatchRequest, run};
use crate::prompt::dispatch::assembler::assemble;
use crate::prompt::inbox::Launcher;
use crate::template::{GitRunner, RealGit};
use crate::workspace::{self, fixture};
use brazen::{Content, Message};
use std::io;
use std::path::Path;

const PARENT: &str = "20260101-p1";

/// A [`Launcher`] that starts nothing: this test asserts on-disk shape,
/// and a real `litany advance` would advance the child underneath it.
struct NoopLauncher;
impl Launcher for NoopLauncher {
    fn launch(&self, _ws: &Path, _agent: &str) -> io::Result<()> {
        Ok(())
    }
}

/// Every `tool_use` id in `history` with no `tool_result` naming it in
/// the immediately following wire message — exactly what a provider
/// validates (§2.5), and what returned `400 "No tool output found"`.
fn unanswered(history: &[Message]) -> Vec<String> {
    let mut out = Vec::new();
    for (i, message) in history.iter().enumerate() {
        for block in &message.content {
            let Content::ToolUse { id, .. } = block else {
                continue;
            };
            let answered = history.get(i + 1).is_some_and(|next| {
                next.content.iter().any(
                    |b| matches!(b, Content::ToolResult { tool_use_id, .. } if tool_use_id == id),
                )
            });
            if !answered {
                out.push(id.clone());
            }
        }
    }
    out
}

#[test]
fn a_child_forked_mid_tool_step_assembles_a_wire_valid_first_history() {
    let (_h, ws) = fixture::workspace();
    fixture::amend_config(&ws, &[("souls/worker.md", "worker soul body\n")]);
    let parent_wt = fixture::spawn_root(&ws, PARENT);
    let g = RealGit::new();

    // The parent's tip at the instant the `dispatch` built-in runs: one
    // settled tool step behind it, and the emitting entry of the step
    // *in progress* — the dispatch's own `tool_use`, whose answering
    // `messages/004-tool.json` cannot commit until the tool returns,
    // which is after this fork.
    std::fs::create_dir_all(parent_wt.join(MESSAGES_DIR)).unwrap();
    for (name, body) in [
        ("001-user.md", "do a thing".to_string()),
        (
            "002-gpt-5.4.json",
            serde_json::to_string(&[tool_use("call_read")]).unwrap(),
        ),
        (
            "003-tool.json",
            serde_json::to_string(&[tool_result("call_read")]).unwrap(),
        ),
        (
            "004-gpt-5.4.json",
            serde_json::to_string(&[
                Content::Text("dispatching a worker".into()),
                tool_use("call_dispatch"),
            ])
            .unwrap(),
        ),
    ] {
        std::fs::write(parent_wt.join(MESSAGES_DIR).join(name), body).unwrap();
    }
    g.run(&parent_wt, &["add", "-A"]).unwrap();
    g.run(&parent_wt, &["commit", "-m", "transcript [p1]"])
        .unwrap();

    // The fork point genuinely carries the defect: the parent's own tree
    // assembles with the dispatch's `tool_use` unanswered.
    assert_eq!(
        unanswered(&assemble(&parent_wt, None).unwrap()),
        vec!["call_dispatch".to_string()],
        "the fork point must be mid-tool-step for this test to mean anything"
    );

    let child = run(
        &ChildDispatchRequest {
            repo: &ws,
            parent_branch: PARENT,
            parent_worktree: &parent_wt,
            role: crate::prompt::compactor::COMPACTOR_ROLE,
            goal: "summarize the repo\n",
            name: None,
            fork_point: None,
            cwd: None,
            pins: crate::prompt::PinnedDocs::none(),
        },
        &g,
        &crate::prompt::SystemClock,
        &crate::prompt::NanoIdGen,
        &NoopLauncher,
        crate::workspace::agent_name::mint::test_rng(),
    )
    .unwrap();

    // The child's first model call is wire-valid: nothing in its history
    // asks for a tool output that will never arrive.
    let child_wt = workspace::agent_worktree(&ws, &child);
    let history = assemble(&child_wt, None).unwrap();
    assert!(
        unanswered(&history).is_empty(),
        "child history still carries an unanswered tool_use: {history:?}"
    );

    // The cut is exactly the unsettled step — the settled pair below it
    // is the child's inherited context and stays (§2.3).
    let listing = g
        .run_capture(
            &workspace::repo_git(&ws),
            &[
                "ls-tree",
                "-r",
                "--name-only",
                &workspace::agent_ref(&child),
            ],
        )
        .unwrap();
    let has = |p: &str| listing.lines().any(|l| l == p);
    assert!(has("messages/001-user.md"), "{listing}");
    assert!(has("messages/002-gpt-5.4.json"), "{listing}");
    assert!(has("messages/003-tool.json"), "{listing}");
    assert!(!has("messages/004-gpt-5.4.json"), "{listing}");

    // The parent's own record is untouched: the deletion is the child's
    // honesty about a step it will never see settle, not a rewrite of
    // the branch where that step does settle (§2.3 immutability).
    let parent_listing = g
        .run_capture(
            &workspace::repo_git(&ws),
            &[
                "ls-tree",
                "-r",
                "--name-only",
                &workspace::agent_ref(PARENT),
            ],
        )
        .unwrap();
    assert!(
        parent_listing
            .lines()
            .any(|l| l == "messages/004-gpt-5.4.json"),
        "{parent_listing}"
    );
}
