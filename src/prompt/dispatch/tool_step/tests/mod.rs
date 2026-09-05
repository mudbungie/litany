//! The tool window's suite (ARCH §3.3): the shared scaffolding — the
//! branch-with-step disk shape, the step [`Resolution`], the
//! never-reached step machinery stubs — and the block-filter contract
//! of [`super::run_tool_calls`]. Each named facet of the window lives
//! in its own submodule below.

use super::Resolved;
use crate::prompt::clock::SystemClock;
use crate::prompt::tool::{ToolCall, ToolExecutor};
use crate::template::{GitRunner, RealGit};
use brazen::Content;
use serde_json::json;
use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use tempfile::TempDir;

/// Context files on the tool result (ARCH §3.3 *Context files ride the
/// next tool result*).
mod context;
/// The binding's host injection at the grant gate (ARCH §3.3).
mod injection;
/// What a role may *call* (ARCH §3.3 declaring is not permitting).
mod permit;
mod policy;
/// The scripted executor and step machinery the window tests share.
mod scripted;
/// The tool-control seam (ARCH §3.3 *Tool control*).
mod seam;
/// The seam's hold-mark lifecycle.
mod seam_hold;
/// The stopped exit's settlement of its own window (ARCH §2.9).
mod settle;
/// The composed side-effect contract (§3.3 commit-per-side-effect).
mod side_effect;

/// A materialized agent worktree on its own branch, carrying the step-1
/// transcript entry, plus the workspace-root step directory — the disk
/// shape [`super::run_tool_calls`] runs against.
fn branch_with_step(ws: &TempDir, agent_id: &str, git: &RealGit) -> (PathBuf, String) {
    let worktree = crate::workspace::agent_worktree(ws.path(), agent_id);
    std::fs::create_dir_all(&worktree).unwrap();
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
    let step_dir_rel = format!("steps/{agent_id}/001");
    std::fs::create_dir_all(ws.path().join(&step_dir_rel)).unwrap();
    (worktree, step_dir_rel)
}

/// A step resolution (§4.3) to run a tool window against. The tool
/// window reads the role, the `tools:` grant (they travel together,
/// [`super::run_tool_calls`]), and the workflow's `tool_output:`
/// policy (§3.3, [`policy`]) — the fixture owns the rest, which no
/// tool call enters.
struct Resolution {
    workflow: crate::config::Workflow,
}

impl Resolution {
    fn new() -> Self {
        Self {
            workflow: crate::config::Workflow::parse(
                "events: {}\n",
                std::path::Path::new("workflow.yaml"),
            )
            .unwrap(),
        }
    }

    fn of<'a>(&'a self, role: &'a str, grant: &'a [String]) -> Resolved<'a> {
        Resolved {
            grant: crate::prompt::dispatch::Grant {
                role,
                tools: grant,
                config_commit: "c0ffee",
            },
            model_id: "claude-sonnet-5",
            provider_row: "anthropic",
            effort: None,
            priority: None,
            soul: "be helpful".into(),
            binary: "bz".into(),
            retry: self.workflow.retry,
            budgets: self.workflow.budgets,
            workflow: &self.workflow,
            workflow_commit: "c0ffee",
            manifest: None,
            expect_handshake: false,
        }
    }
}

/// The step machinery a tool window never reaches: the adapter, the
/// retry sleeper, and the exit launcher.
struct NoAdapter;
impl crate::prompt::adapter::AdapterRunner for NoAdapter {
    fn run(
        &self,
        _b: &std::ffi::OsString,
        _a: &[&str],
        _s: &[u8],
        _o: &mut dyn FnMut(&[u8]) -> std::io::Result<()>,
    ) -> std::io::Result<Vec<u8>> {
        unreachable!("adapter is never reached")
    }
}
struct NoSleeper;
impl crate::prompt::dispatch::Sleeper for NoSleeper {
    fn sleep(&self, _d: std::time::Duration) {
        unreachable!("sleeper is never reached")
    }
}
struct NoLauncher;
impl crate::prompt::inbox::Launcher for NoLauncher {
    fn launch(&self, _ws: &std::path::Path, _agent: &str) -> std::io::Result<()> {
        unreachable!("launcher is never reached")
    }
}

/// A recording executor: run_tool_calls hands it only `tool_use` blocks
/// (the loop's block filter) together with the governing workflow's
/// `tool_output:` policy (§3.3 bounded projection) — both observable.
struct Recorder(std::cell::RefCell<Vec<(String, Option<crate::config::ToolOutputBound>)>>);

impl ToolExecutor for Recorder {
    fn execute(
        &self,
        call: ToolCall<'_>,
        _step_dir: &std::path::Path,
        _stop: &AtomicBool,
        output_bound: Option<crate::config::ToolOutputBound>,
    ) -> Result<crate::prompt::tool::ToolOutcome, crate::prompt::tool::ExecError> {
        self.0
            .borrow_mut()
            .push((call.name.to_string(), output_bound));
        Ok(crate::prompt::tool::ToolOutcome {
            content: b"ok".to_vec(),
            is_error: false,
        })
    }
}

#[test]
fn run_tool_calls_executes_only_the_tool_use_blocks() {
    // A model's output interleaves prose with its tool calls (§3.3);
    // only the `tool_use` blocks reach the executor, and the loop
    // reports "continue" (no stop observed).
    let agent_id = "agent-6f1b";
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
        Content::Text("running the check".into()),
        Content::ToolUse {
            id: "t1".into(),
            name: "bash".into(),
            input: json!({"command": "true"}),
            signature: None,
        },
    ];
    let resolution = Resolution::new();
    let grant = ["bash".to_string()];
    let window = super::run_tool_calls(
        ws.path(),
        &worktree,
        agent_id,
        &resolution.of(crate::prompt::WORKER_ROLE, &grant),
        &step_dir_rel,
        &content,
        &deps,
    )
    .unwrap();
    assert!(
        matches!(window, super::ToolWindow::Completed),
        "no stop, no hold: the window completes"
    );
    // One call reached the executor; this workflow declares no
    // `tool_output:` block, so the projection policy travels as absent.
    assert_eq!(*recorder.0.borrow(), vec![("bash".to_string(), None)]);
    // The single tool result was committed as the next transcript entry.
    assert!(worktree.join("messages/002-tool.json").exists());
}
