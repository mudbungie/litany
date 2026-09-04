//! Declaring is not permitting, for every role (ARCH §3.3, §4.3).
//!
//! A branch inherits its dispatcher's transcript by fork (§2.3), and the
//! request array is closed over the history it ships (§3.3) — so a role
//! routinely *declares* tools its `providers.yaml` grant omits. The gate
//! asserted here is the second fact: what it may **call** is its grant
//! plus its procedure's injected pair, and nothing else reaches the
//! executor. Before bl-5a1f only the compactor was gated, so any other
//! role could run whatever its dispatcher had used.

use super::{NoAdapter, NoLauncher, NoSleeper, Recorder, Resolution, branch_with_step};
use crate::prompt::clock::SystemClock;
use crate::prompt::compactor::COMPACTOR_ROLE;
use crate::prompt::dispatch::tool_step::{refusal, run_tool_calls};
use crate::template::RealGit;
use brazen::Content;
use serde_json::json;
use std::sync::atomic::AtomicBool;
use tempfile::TempDir;

#[test]
fn an_ungranted_tool_is_declined_in_band_and_the_granted_one_still_runs() {
    // The fleet's read-only sensor: granted [slack_read, message], forked
    // from a dispatcher that used `bash`, so `bash` rides its request.
    let agent_id = "agent-5a1f";
    let ws = TempDir::new().unwrap();
    let git = RealGit::new();
    let (worktree, step_dir_rel) = branch_with_step(&ws, agent_id, &git);

    let recorder = Recorder(std::cell::RefCell::new(Vec::new()));
    let stop = AtomicBool::new(false);
    let clock = SystemClock;
    let id_gen = crate::prompt::NanoIdGen;
    let cfg = TempDir::new().unwrap();
    let deps = crate::prompt::Deps {
        adapter: &NoAdapter,
        sleeper: &NoSleeper,
        git: &git,
        clock: &clock,
        id_gen: &id_gen,
        tool_executor: &recorder,
        config_root: cfg.path(),
        data_root: cfg.path(),
        adapter_target: None,
        stop: &stop,
        launcher: &NoLauncher,
        rng: crate::workspace::agent_name::mint::test_rng(),
    };
    let content = vec![
        Content::ToolUse {
            id: "t_denied".into(),
            name: "bash".into(),
            input: json!({"command": "rm -rf ."}),
            signature: None,
        },
        Content::ToolUse {
            id: "t_granted".into(),
            name: "slack_read".into(),
            input: json!({"channel": "#ops"}),
            signature: None,
        },
    ];
    let grant = ["slack_read".to_string(), "message".to_string()];
    let resolution = Resolution::new();
    let window = run_tool_calls(
        ws.path(),
        &worktree,
        agent_id,
        &resolution.of("sensor", &grant),
        &step_dir_rel,
        &content,
        &deps,
    )
    .unwrap();

    assert!(matches!(
        window,
        crate::prompt::dispatch::tool_step::ToolWindow::Completed
    ));
    // Only the granted tool reached the executor.
    assert_eq!(*recorder.0.borrow(), vec![("slack_read".to_string(), None)]);
    // The decline landed as an ordinary transcript entry naming the grant.
    let entry = std::fs::read_to_string(worktree.join("messages/002-tool.json")).unwrap();
    let blocks: Vec<Content> = serde_json::from_str(&entry).unwrap();
    let Content::ToolResult {
        tool_use_id,
        content,
        is_error,
    } = &blocks[0]
    else {
        panic!("expected a tool_result, got {:?}", blocks[0]);
    };
    assert_eq!(tool_use_id, "t_denied");
    assert!(is_error, "the decline is an error result");
    let Content::Text(text) = &content[0] else {
        panic!("the decline is text");
    };
    assert!(text.contains("\"bash\""), "{text}");
    assert!(text.contains("not callable by a sensor"), "{text}");
    assert!(text.contains("slack_read, message"), "{text}");
    // The granted tool call's own result is the next entry.
    assert!(worktree.join("messages/003-tool.json").exists());
}

/// The compactor's procedure injection, as the composer and the gate
/// both read it (§2.7, `tools::injected`).
fn compactor_pair() -> Vec<crate::prompt::tool::inject::InjectedTool> {
    crate::prompt::compactor::builtin_tool_schemas(COMPACTOR_ROLE)
}

#[test]
fn the_decline_names_an_empty_toolset_when_the_role_grants_none() {
    // A role with neither a `tools:` grant nor an injected pair: the
    // decline still names its toolset rather than trailing off.
    let declined = refusal("watcher", &[], &[], "bash").expect("nothing is callable");
    assert!(
        declined.contains("The watcher toolset is empty"),
        "{declined}"
    );
}

#[test]
fn a_compactor_calls_its_injected_pair_and_nothing_else() {
    // §2.7 through the general rule: the compactor's `tools:` grant is
    // empty in every shipped config, so its effective toolset *is* the
    // injected pair — deletion-only, with no executor-side special case.
    assert_eq!(
        refusal(COMPACTOR_ROLE, &[], &compactor_pair(), "write_summary"),
        None
    );
    assert_eq!(
        refusal(COMPACTOR_ROLE, &[], &compactor_pair(), "mark_for_deletion"),
        None
    );
    let declined = refusal(COMPACTOR_ROLE, &[], &compactor_pair(), "bash")
        .expect("an inherited tool is declined");
    assert!(
        declined.contains("not callable by a compactor"),
        "{declined}"
    );
    assert!(
        declined.contains("write_summary, mark_for_deletion"),
        "{declined}"
    );
}
