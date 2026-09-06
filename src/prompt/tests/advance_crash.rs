//! §6 crash settlement (bl-4187): the drive boundary settles a
//! markless unpaired trailing window *before* delivery, so an ordinary
//! deposit revives a crashed branch. The buried form — mail already
//! behind the orphan — stays the loud decline (`advance_edges`).

use super::advance::{AGENT, RecLauncher, model_entry, worker_config, workspace_with_tail};
use super::fixtures::*;
use crate::prompt::dispatch::advance::{AdvanceOutcome, run};
use crate::prompt::inbox;
use crate::template::RealGit;
use brazen::Content;

/// A crash corpse: the assistant entry committed, its `tool_use` never
/// answered, no hold mark, no mail behind it — the executor died
/// mid-window and its lease was kernel-released.
fn crashed_tail() -> Vec<(&'static str, String)> {
    vec![
        ("001-user.md", "hi".to_string()),
        (
            "002-claude-sonnet-5.json",
            model_entry(&[
                Content::Text("running".into()),
                Content::ToolUse {
                    id: "t1".into(),
                    name: "bash".into(),
                    input: serde_json::json!({"command": "true"}),
                    signature: None,
                },
            ]),
        ),
    ]
}

#[test]
fn a_deposit_revives_a_crashed_window_through_settlement() {
    let (ws, wt) = workspace_with_tail(&crashed_tail());
    let (clock, id) = (FixedClock::default(), FixedIdGen);
    inbox::deposit(ws.path(), AGENT, "user", "hello?", &clock, &RealGit::new()).unwrap();
    let adapter = StubAdapter::scripted([StubAdapter::reply_ok(&happy_response_bytes())]);
    let (sleeper, git) = (StubSleeper::default(), StubGit::ok());
    let tools = StubToolExecutor::ok();
    let rec = RecLauncher::default();
    let mut deps = valid_deps(&adapter, &sleeper, &git, &clock, &id, &tools, ws.path());
    deps.launcher = &rec;
    let out = run(ws.path(), AGENT, None, &deps, &mut || Ok(worker_config())).unwrap();
    assert!(matches!(out, AdvanceOutcome::Terminal), "got {out:?}");
    // Ordering is the invariant (§2.3 pairing is positional): the
    // settlement landed FIRST (003), the mail behind it (004), and the
    // revived step answered (005).
    let settlement = std::fs::read_to_string(wt.join("messages/003-tool.json")).unwrap();
    assert!(settlement.contains("t1"), "{settlement}");
    assert!(
        settlement.contains("died before recording an outcome"),
        "{settlement}"
    );
    let delivered = std::fs::read_to_string(wt.join("messages/004-user.md")).unwrap();
    assert!(delivered.contains("hello?"), "{delivered}");
    assert!(wt.join("messages/005-claude-sonnet-5.json").exists());
}

#[test]
fn a_partially_answered_crash_settles_only_the_unanswered_ids() {
    // The window committed t1's result, then died before t2's: the tail
    // composes tool-side, and only t2 is settled — results that landed
    // keep the one entry they already have (idempotence, PRINCIPLES
    // single source of truth).
    let entries = vec![
        ("001-user.md", "hi".to_string()),
        (
            "002-claude-sonnet-5.json",
            model_entry(&[
                Content::ToolUse {
                    id: "t1".into(),
                    name: "bash".into(),
                    input: serde_json::json!({"command": "true"}),
                    signature: None,
                },
                Content::ToolUse {
                    id: "t2".into(),
                    name: "bash".into(),
                    input: serde_json::json!({"command": "false"}),
                    signature: None,
                },
            ]),
        ),
        (
            "003-tool.json",
            model_entry(&[Content::ToolResult {
                tool_use_id: "t1".into(),
                content: vec![Content::Text("done".into())],
                is_error: false,
            }]),
        ),
    ];
    let (ws, wt) = workspace_with_tail(&entries);
    let (clock, id) = (FixedClock::default(), FixedIdGen);
    inbox::deposit(ws.path(), AGENT, "user", "and?", &clock, &RealGit::new()).unwrap();
    let adapter = StubAdapter::scripted([StubAdapter::reply_ok(&happy_response_bytes())]);
    let (sleeper, git) = (StubSleeper::default(), StubGit::ok());
    let tools = StubToolExecutor::ok();
    let rec = RecLauncher::default();
    let mut deps = valid_deps(&adapter, &sleeper, &git, &clock, &id, &tools, ws.path());
    deps.launcher = &rec;
    let out = run(ws.path(), AGENT, None, &deps, &mut || Ok(worker_config())).unwrap();
    assert!(matches!(out, AdvanceOutcome::Terminal), "got {out:?}");
    let settlement = std::fs::read_to_string(wt.join("messages/004-tool.json")).unwrap();
    assert!(settlement.contains("\"t2\""), "{settlement}");
    assert!(!settlement.contains("\"t1\""), "{settlement}");
    assert!(wt.join("messages/005-user.md").exists());
}

#[test]
fn a_never_stepped_user_tail_settles_nothing_and_steps_normally() {
    // No assistant entry anywhere: the settlement's window search finds
    // nothing and the hop is the ordinary first step off a user tail.
    let entries = vec![("001-user.md", "hi".to_string())];
    let (ws, wt) = workspace_with_tail(&entries);
    let (clock, id) = (FixedClock::default(), FixedIdGen);
    let adapter = StubAdapter::scripted([StubAdapter::reply_ok(&happy_response_bytes())]);
    let (sleeper, git) = (StubSleeper::default(), StubGit::ok());
    let tools = StubToolExecutor::ok();
    let rec = RecLauncher::default();
    let mut deps = valid_deps(&adapter, &sleeper, &git, &clock, &id, &tools, ws.path());
    deps.launcher = &rec;
    let out = run(ws.path(), AGENT, None, &deps, &mut || Ok(worker_config())).unwrap();
    assert!(matches!(out, AdvanceOutcome::Terminal), "got {out:?}");
    // No settlement entry: the answer landed directly at 002.
    assert!(wt.join("messages/002-claude-sonnet-5.json").exists());
    assert!(!wt.join("messages/002-tool.json").exists());
}
