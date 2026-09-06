//! The compactor's two toolset facts, on the §6 hop: what its request
//! **declares** and what it may **call** (ARCH §2.7, §3.3).
//!
//! A compactor forks with the dispatching branch's transcript in its tree
//! (§2.3 *Fork and inheritance*), and the checkpoint clock is read closing
//! a tool step (§6) — so the history it inherits routinely ends on a
//! `tool_use` / `tool_result` pair for a tool that is not one of its two.
//! The request must name that tool or the provider refuses the call
//! outright; the compactor must still not be able to *run* it, or §2.7's
//! deletion-only guarantee stops being structural. Both are asserted here
//! against the disk record — the step's `request.json` and the committed
//! transcript entry — never a carried payload.

use super::advance::{AGENT, RecLauncher, model_entry, worker_config, workspace_with_tail};
use super::fixtures::*;
use crate::prompt::dispatch::advance::run;
use crate::prompt::inbox;
use crate::prompt::resolve::WorkerConfig;
use brazen::{Content, FinishReason};
use serde_json::json;
use std::path::Path;

/// The shape a dispatched compactor resolves (§6 role-aware resolution).
fn compactor_config() -> WorkerConfig {
    WorkerConfig {
        role: "compactor".into(),
        // The shipped compactor row grants no tools (§4.3): its toolset
        // is the injected pair alone (§2.7).
        tools: vec![],
        ..worker_config()
    }
}

/// The `bash` input schema the dispatching branch's config committed, and
/// which the compactor's fork inherits under `descriptions/**` (§3.3).
const BASH_SCHEMA: &str = r#"{"type":"object","properties":{"command":{"type":"string"}}}"#;

/// A transcript tail that used `bash`: the pair the checkpoint clock fires
/// on top of (§6 — the clock is read closing a tool step).
fn tail_that_used_bash() -> Vec<(&'static str, String)> {
    vec![
        ("001-user.md", "do a thing".to_string()),
        (
            "002-claude-sonnet-5.json",
            model_entry(&[Content::ToolUse {
                id: "toolu_bash".into(),
                name: "bash".into(),
                input: json!({"command": "echo hello-from-tool"}),
                signature: None,
            }]),
        ),
        (
            "003-tool.json",
            model_entry(&[Content::ToolResult {
                tool_use_id: "toolu_bash".into(),
                content: vec![Content::Text("hello-from-tool\n".into())],
                is_error: false,
            }]),
        ),
    ]
}

/// Commit `descriptions/tools/bash.json` into the compactor's inherited
/// worktree, as the fork off the dispatching branch carries it.
fn inherit_bash_schema(worktree: &Path) {
    let dir = worktree.join("descriptions/tools");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("bash.json"), BASH_SCHEMA).unwrap();
}

/// The declared tool names of the step's `request.json` (§3.3).
fn declared_names(workspace: &Path) -> Vec<String> {
    let req: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(workspace.join(format!("steps/{AGENT}/001/request.json")))
            .unwrap(),
    )
    .unwrap();
    req["tools"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["name"].as_str().unwrap().to_string())
        .collect()
}

#[test]
fn a_compactor_declares_the_inherited_transcripts_tools_alongside_its_own() {
    // The bl-f021 repro: the inherited history references `bash`, which
    // no `tools:` list of the compactor role declares. Before the fix the
    // request shipped that exchange with a two-entry `tools` array and the
    // provider refused the call ("tool accepts only text content"); now
    // the array is closed over the history it ships.
    let (ws, wt) = workspace_with_tail(&tail_that_used_bash());
    inherit_bash_schema(&wt);
    let (clock, id) = (FixedClock::default(), FixedIdGen);
    inbox::deposit(ws.path(), AGENT, "user", "compact", &clock).unwrap();
    let adapter = StubAdapter::scripted([StubAdapter::reply_ok(&happy_response_bytes())]);
    let (sleeper, git, tools) = (
        StubSleeper::default(),
        StubGit::ok(),
        StubToolExecutor::ok(),
    );
    let rec = RecLauncher::default();
    let mut deps = valid_deps(&adapter, &sleeper, &git, &clock, &id, &tools, ws.path());
    deps.launcher = &rec;
    run(
        ws.path(),
        AGENT,
        None,
        &deps,
        &mut || Ok(compactor_config()),
    )
    .unwrap();

    let names = declared_names(ws.path());
    assert!(names.contains(&"write_summary".to_string()), "{names:?}");
    assert!(
        names.contains(&"mark_for_deletion".to_string()),
        "{names:?}"
    );
    assert!(names.contains(&"bash".to_string()), "{names:?}");

    // The transcript is not rewritten to fit the declaration (§2.3): the
    // inherited pair rides the request verbatim.
    let req: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(ws.path().join(format!("steps/{AGENT}/001/request.json")))
            .unwrap(),
    )
    .unwrap();
    assert!(
        req["messages"].to_string().contains("toolu_bash"),
        "{}",
        req["messages"]
    );
    // …and the inherited name is declared as one the compactor may NOT
    // call (bl-9c1d): the door's own refusal as its description and an
    // opaque schema, not the inherited definition. The entry exists to
    // make the history legible, and the compactor spent a model call per
    // checkpoint discovering that the hard way.
    let bash = req["tools"]
        .as_array()
        .unwrap()
        .iter()
        .find(|t| t["name"] == "bash")
        .unwrap();
    assert_eq!(bash["input_schema"], serde_json::json!({"type": "object"}));
    let description = bash["description"].as_str().unwrap_or_default();
    assert!(
        description.contains("not callable by a compactor"),
        "{description}"
    );
}

#[test]
fn a_worker_hop_declares_only_its_own_tools_when_the_history_used_none() {
    // The plain path is unchanged: no `tool_use` in the history, no
    // built-in injection, so nothing is appended and the array stays empty.
    let (ws, _wt) = workspace_with_tail(&super::advance::terminal_tail());
    let (clock, id) = (FixedClock::default(), FixedIdGen);
    inbox::deposit(ws.path(), AGENT, "user", "carry on", &clock).unwrap();
    let adapter = StubAdapter::scripted([StubAdapter::reply_ok(&happy_response_bytes())]);
    let (sleeper, git, tools) = (
        StubSleeper::default(),
        StubGit::ok(),
        StubToolExecutor::ok(),
    );
    let rec = RecLauncher::default();
    let mut deps = valid_deps(&adapter, &sleeper, &git, &clock, &id, &tools, ws.path());
    deps.launcher = &rec;
    run(ws.path(), AGENT, None, &deps, &mut || Ok(worker_config())).unwrap();

    assert!(declared_names(ws.path()).is_empty());
}

#[test]
fn a_compactor_calling_an_inherited_tool_is_declined_not_executed() {
    // Declared is not callable (§2.7): the compactor can see `bash`
    // in its request, and reaching for it yields an `is_error`
    // `tool_result` naming its own two tools — the executor is never
    // entered, so the deletion-only guarantee stays structural.
    let (ws, wt) = workspace_with_tail(&tail_that_used_bash());
    inherit_bash_schema(&wt);
    let (clock, id) = (FixedClock::default(), FixedIdGen);
    inbox::deposit(ws.path(), AGENT, "user", "compact", &clock).unwrap();
    let reaches_for_bash = stream_of(
        FinishReason::ToolUse,
        &[Block::ToolUse {
            id: "toolu_reach",
            name: "bash",
            input: json!({"command": "rm -rf ."}),
        }],
    );
    let adapter = StubAdapter::scripted([StubAdapter::reply_ok(&reaches_for_bash)]);
    let (sleeper, git, tools) = (
        StubSleeper::default(),
        StubGit::ok(),
        StubToolExecutor::ok(),
    );
    let rec = RecLauncher::default();
    let mut deps = valid_deps(&adapter, &sleeper, &git, &clock, &id, &tools, ws.path());
    deps.launcher = &rec;
    run(
        ws.path(),
        AGENT,
        None,
        &deps,
        &mut || Ok(compactor_config()),
    )
    .unwrap();

    // The executor never saw the call.
    assert!(
        tools.invocations.borrow().is_empty(),
        "a compactor's foreign tool must not execute: {:?}",
        tools.invocations.borrow()
    );
    // The decline reached the model as an ordinary transcript entry.
    let entry = std::fs::read_to_string(wt.join("messages/006-tool.json")).unwrap();
    let blocks: Vec<Content> = serde_json::from_str(&entry).unwrap();
    match &blocks[0] {
        Content::ToolResult {
            tool_use_id,
            content,
            is_error,
        } => {
            assert_eq!(tool_use_id, "toolu_reach");
            assert!(is_error, "the decline is an error result");
            let Content::Text(text) = &content[0] else {
                panic!("decline is text");
            };
            assert!(text.contains("not callable by a compactor"), "{text}");
            assert!(text.contains("write_summary"), "{text}");
        }
        other => panic!("expected a tool_result, got {other:?}"),
    }
}
