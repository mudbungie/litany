//! The composed side-effect contract: what a tool writes lands in the
//! calling agent's worktree (ARCH §3.3 *Working directory*) and rides
//! the same commit as its `tool_result` entry (§2.3, §3.3
//! commit-per-side-effect — [`super::transcript::commit_tool`] stages
//! `-A`).
//!
//! The two halves are what [`super::run_tool_calls`] pairs for every
//! tool call; asserting them over the *production* executor and real git is
//! the only way to see that the pairing actually captures the write —
//! a tool left in the harness's inherited cwd wrote its file outside
//! every worktree, where `git add -A` could never see it (bl-2503).

use super::Resolved;
use super::transcript::commit_tool;
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

/// The binding's host injection at the grant gate (ARCH §3.3).
mod injection;
/// The multi-tool envelope (ARCH §3.3 *The multi-tool*).
mod multi;
mod multi_faults;
mod multi_parallel;
/// The tool-control seam inside a multi-tool envelope.
mod multi_seam;
/// What a role may *call* (ARCH §3.3 declaring is not permitting).
mod permit;
mod policy;
/// The tool-control seam (ARCH §3.3 *Tool control*).
mod seam;
/// The seam's hold-mark lifecycle.
mod seam_hold;
/// The stopped exit's settlement of its own window (ARCH §2.9).
mod settle;

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
            soul: "be helpful".into(),
            binary: "bz".into(),
            retry: self.workflow.retry,
            budgets: self.workflow.budgets,
            workflow: &self.workflow,
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
