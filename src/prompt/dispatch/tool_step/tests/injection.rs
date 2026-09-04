//! The grant gate over a host-injected tool (ARCH §3.3 *Host-injected
//! tools*, `docs/DESIGN_TOOL_INJECTION.md`).
//!
//! A tool the binding injects is declared by no `providers.yaml` and
//! lives in no pool, so without the union here it would be declared to
//! the model and then refused the moment the model called it — the
//! failure the seam's one-object shape exists to make impossible. The
//! union is read off the executor, which is also what will answer the
//! call, so the two halves cannot disagree.

use super::{NoAdapter, NoLauncher, NoSleeper, Resolution, branch_with_step};
use crate::prompt::clock::SystemClock;
use crate::prompt::dispatch::tool_step::{ToolWindow, run_tool_calls};
use crate::prompt::tool::inject::{InjectedTool, RoutedCall, RoutedCapture, ToolInjection};
use crate::prompt::tool::{ExecError, ToolCall, ToolExecutor, ToolOutcome};
use crate::template::RealGit;
use brazen::Content;
use serde_json::json;
use std::cell::RefCell;
use std::sync::atomic::AtomicBool;
use tempfile::TempDir;

/// An executor carrying a host injection: it declares `teleop` and
/// answers it, recording what reached it. Stands in for the production
/// `SpawnTool` wrapped around a binding's `Fx::tool_injection` — the
/// window only ever sees the executor, so this is the same seam.
struct Hosted(RefCell<Vec<String>>);

impl ToolInjection for Hosted {
    fn tools(&self, _workspace: &std::path::Path, _agent: &str) -> Vec<InjectedTool> {
        vec![InjectedTool {
            name: "teleop".into(),
            input_schema: json!({"type": "object"}),
            description: None,
        }]
    }

    fn route(&self, _call: RoutedCall<'_>) -> RoutedCapture {
        unreachable!("this stub answers at `execute`, not through a spawn path")
    }
}

impl ToolExecutor for Hosted {
    fn execute(
        &self,
        call: ToolCall<'_>,
        _step_dir: &std::path::Path,
        _stop: &AtomicBool,
        _bound: Option<crate::config::ToolOutputBound>,
    ) -> Result<ToolOutcome, ExecError> {
        self.0.borrow_mut().push(call.name.to_string());
        Ok(ToolOutcome {
            content: b"Exit code: 0\nrouted".to_vec(),
            is_error: false,
        })
    }

    fn injected(&self, workspace: &std::path::Path, agent: &str) -> Vec<InjectedTool> {
        ToolInjection::tools(self, workspace, agent)
    }
}

#[test]
fn an_injected_tool_is_callable_by_a_role_that_grants_nothing() {
    // The whole point: `teleop` is in no grant and no pool, and the
    // window still lets it through — while an ungranted, uninjected name
    // beside it is declined exactly as before.
    let agent_id = "agent-9001";
    let ws = TempDir::new().unwrap();
    let git = RealGit::new();
    let (worktree, step_dir_rel) = branch_with_step(&ws, agent_id, &git);

    let hosted = Hosted(RefCell::new(Vec::new()));
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
        tool_executor: &hosted,
        config_root: cfg.path(),
        data_root: cfg.path(),
        adapter_target: None,
        stop: &stop,
        launcher: &NoLauncher,
        rng: crate::workspace::agent_name::mint::test_rng(),
    };
    let content = vec![
        Content::ToolUse {
            id: "t_host".into(),
            name: "teleop".into(),
            input: json!({"do": "thing"}),
            signature: None,
        },
        Content::ToolUse {
            id: "t_denied".into(),
            name: "bash".into(),
            input: json!({"command": "true"}),
            signature: None,
        },
    ];
    let resolution = Resolution::new();
    let window = run_tool_calls(
        ws.path(),
        &worktree,
        agent_id,
        &resolution.of("watcher", &[]),
        &step_dir_rel,
        &content,
        &deps,
    )
    .unwrap();

    assert!(matches!(window, ToolWindow::Completed));
    assert_eq!(*hosted.0.borrow(), vec!["teleop".to_string()]);

    // The injected tool's result is an ordinary transcript entry.
    let entry = std::fs::read_to_string(worktree.join("messages/002-tool.json")).unwrap();
    let blocks: Vec<Content> = serde_json::from_str(&entry).unwrap();
    let Content::ToolResult { is_error, .. } = &blocks[0] else {
        panic!("expected a tool_result, got {:?}", blocks[0]);
    };
    assert!(!is_error);

    // The uninjected, ungranted name is still declined in band, and the
    // decline names the injected tool as part of the effective toolset.
    let entry = std::fs::read_to_string(worktree.join("messages/003-tool.json")).unwrap();
    let blocks: Vec<Content> = serde_json::from_str(&entry).unwrap();
    let Content::ToolResult {
        content, is_error, ..
    } = &blocks[0]
    else {
        panic!("expected a tool_result, got {:?}", blocks[0]);
    };
    assert!(is_error);
    let Content::Text(text) = &content[0] else {
        panic!("the decline is text");
    };
    assert!(text.contains("not callable by a watcher"), "{text}");
    assert!(text.contains("The watcher toolset is teleop"), "{text}");
}
