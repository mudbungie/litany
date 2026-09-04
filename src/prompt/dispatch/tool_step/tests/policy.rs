//! The §3.3 bounded-projection policy travels from the governing
//! workflow to the executor (bl-d5fa): `run_tool_calls` reads
//! `tool_output:` off the step's [`super::Resolved`] workflow — the one
//! home the policy has (§2.2 control from the config commit) — and
//! hands it to every `execute`. Sibling file so `mod.rs` stays under
//! the 300-line cap.

use super::{NoAdapter, NoLauncher, NoSleeper, Recorder, Resolution, branch_with_step};
use crate::config::ToolOutputBound;
use crate::template::RealGit;
use brazen::Content;
use serde_json::json;
use std::sync::atomic::AtomicBool;
use tempfile::TempDir;

#[test]
fn the_workflow_tool_output_policy_reaches_the_executor() {
    let agent_id = "agent-d5fa";
    let ws = TempDir::new().unwrap();
    let git = RealGit::new();
    let (worktree, step_dir_rel) = branch_with_step(&ws, agent_id, &git);

    let mut resolution = Resolution::new();
    resolution.workflow = crate::config::Workflow::parse(
        "events: {}\ntool_output:\n  head_bytes: 4\n  tail_bytes: 4\n",
        std::path::Path::new("workflow.yaml"),
    )
    .unwrap();

    let recorder = Recorder(std::cell::RefCell::new(Vec::new()));
    let stop = AtomicBool::new(false);
    let clock = crate::prompt::clock::SystemClock;
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
    let content = vec![Content::ToolUse {
        id: "t1".into(),
        name: "bash".into(),
        input: json!({"command": "true"}),
        signature: None,
    }];
    let grant = ["bash".to_string()];
    super::super::run_tool_calls(
        ws.path(),
        &worktree,
        agent_id,
        &resolution.of(crate::prompt::WORKER_ROLE, &grant),
        &step_dir_rel,
        &content,
        &deps,
    )
    .unwrap();
    assert_eq!(
        *recorder.0.borrow(),
        vec![(
            "bash".to_string(),
            Some(ToolOutputBound {
                head_bytes: 4,
                tail_bytes: 4,
            })
        )]
    );
}
