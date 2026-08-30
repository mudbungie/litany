//! The tool-control seam end-to-end at the window (ARCH §3.3 *Tool
//! control*): pass and refuse against real fixture controls, the
//! grant-before-control ordering, and the fail-closed faults. The
//! hold-mark lifecycle lives in [`super::seam_hold`] (300-line cap).

use super::{NoAdapter, NoLauncher, NoSleeper, Recorder, Resolution, branch_with_step};
use crate::prompt::dispatch::tool_step::{ToolWindow, run_tool_calls};
use crate::prompt::{Deps, Error};
use crate::template::{GitRunner, RealGit};
use brazen::Content;
use serde_json::json;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use tempfile::TempDir;

/// An executable fixture control beside the workspace.
pub(super) fn control_script(dir: &Path, body: &str) -> PathBuf {
    let path = dir.join("fixture-control.sh");
    std::fs::write(&path, format!("#!/bin/sh\n{body}\n")).unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
    path
}

/// A [`Resolution`] whose workflow names `control` as the tool control.
pub(super) fn gated(resolution: &mut Resolution, control: &Path) {
    resolution.workflow = crate::config::Workflow::parse(
        &format!(
            "events: {{}}\ntool_control:\n  command: {}\n",
            control.display()
        ),
        Path::new("workflow.yaml"),
    )
    .unwrap();
}

pub(super) fn tool_use(id: &str, name: &str) -> Content {
    Content::ToolUse {
        id: id.into(),
        name: name.into(),
        input: json!({"command": "true"}),
        signature: None,
    }
}

/// A bare `repo.git` beside the worktree so the hold mark has its home
/// (§2.2 — marks live in the workspace repo, not the branch worktree).
pub(super) fn found_mark_repo(ws: &Path) {
    RealGit::new()
        .run(ws, &["init", "--bare", "repo.git"])
        .unwrap();
}

pub(super) struct Rig {
    pub(super) ws: TempDir,
    pub(super) worktree: PathBuf,
    step_dir_rel: String,
    recorder: Recorder,
    pub(super) stop: AtomicBool,
    clock: crate::prompt::clock::SystemClock,
    id_gen: crate::prompt::NanoIdGen,
    cfg: TempDir,
}

impl Rig {
    pub(super) fn new(agent_id: &str) -> Self {
        let ws = TempDir::new().unwrap();
        let git = RealGit::new();
        let (worktree, step_dir_rel) = branch_with_step(&ws, agent_id, &git);
        Self {
            ws,
            worktree,
            step_dir_rel,
            recorder: Recorder(std::cell::RefCell::new(Vec::new())),
            stop: AtomicBool::new(false),
            clock: crate::prompt::clock::SystemClock,
            id_gen: crate::prompt::NanoIdGen,
            cfg: TempDir::new().unwrap(),
        }
    }

    fn deps<'a>(&'a self, git: &'a dyn GitRunner) -> Deps<'a> {
        Deps {
            adapter: &NoAdapter,
            sleeper: &NoSleeper,
            git,
            clock: &self.clock,
            id_gen: &self.id_gen,
            tool_executor: &self.recorder,
            config_root: self.cfg.path(),
            adapter_target: None,
            stop: &self.stop,
            launcher: &NoLauncher,
            rng: crate::workspace::agent_name::mint::test_rng(),
        }
    }

    pub(super) fn run(
        &self,
        agent_id: &str,
        resolution: &Resolution,
        content: &[Content],
        git: &dyn GitRunner,
    ) -> Result<ToolWindow, Error> {
        let grant = ["bash".to_string(), "read_file".to_string()];
        run_tool_calls(
            self.ws.path(),
            &self.worktree,
            agent_id,
            &resolution.of(crate::prompt::WORKER_ROLE, &grant),
            &self.step_dir_rel,
            content,
            &self.deps(git),
        )
    }

    pub(super) fn executed(&self) -> Vec<String> {
        self.recorder
            .0
            .borrow()
            .iter()
            .map(|(n, _)| n.clone())
            .collect()
    }
}

#[test]
fn a_pass_verdict_enters_the_executor_unchanged() {
    let rig = Rig::new("agent-pass");
    let control = control_script(rig.ws.path(), "echo '{\"verdict\":\"pass\"}'");
    let mut resolution = Resolution::new();
    gated(&mut resolution, &control);
    let git = RealGit::new();
    let window = rig
        .run("agent-pass", &resolution, &[tool_use("t1", "bash")], &git)
        .unwrap();
    assert!(matches!(window, ToolWindow::Completed));
    assert_eq!(rig.executed(), vec!["bash"]);
    assert!(rig.worktree.join("messages/002-tool.json").exists());
}

#[test]
fn a_refuse_verdict_declines_in_band_and_the_window_continues() {
    // The control refuses `bash` and passes everything else: the refusal
    // is an ordinary `is_error` entry carrying the reason, the executor
    // is never entered for it, and the next block still runs.
    let rig = Rig::new("agent-refuse");
    let control = control_script(
        rig.ws.path(),
        "if grep -q '\"name\":\"bash\"'; then \
           echo '{\"verdict\":\"refuse\",\"reason\":\"shell is under review\"}'; \
         else echo '{\"verdict\":\"pass\"}'; fi",
    );
    let mut resolution = Resolution::new();
    gated(&mut resolution, &control);
    let git = RealGit::new();
    let window = rig
        .run(
            "agent-refuse",
            &resolution,
            &[tool_use("t1", "bash"), tool_use("t2", "read_file")],
            &git,
        )
        .unwrap();
    assert!(matches!(window, ToolWindow::Completed));
    assert_eq!(rig.executed(), vec!["read_file"]);
    let entry = std::fs::read_to_string(rig.worktree.join("messages/002-tool.json")).unwrap();
    let blocks: Vec<Content> = serde_json::from_str(&entry).unwrap();
    let Content::ToolResult {
        tool_use_id,
        content,
        is_error,
    } = &blocks[0]
    else {
        panic!("expected a tool_result, got {:?}", blocks[0]);
    };
    assert_eq!(tool_use_id, "t1");
    assert!(is_error);
    let Content::Text(text) = &content[0] else {
        panic!("the decline is text");
    };
    assert!(
        text.contains("refused by the workflow's tool control"),
        "{text}"
    );
    assert!(text.contains("shell is under review"), "{text}");
    assert!(rig.worktree.join("messages/003-tool.json").exists());
}

#[test]
fn an_ungranted_tool_is_declined_before_the_control_is_consulted() {
    // Grants are structure, controls are policy: the grant gate decline
    // never spawns the control (the fixture would leave a footprint).
    let rig = Rig::new("agent-order");
    let control = control_script(
        rig.ws.path(),
        "touch \"$LITANY_CONV_REPO/consulted\"\necho '{\"verdict\":\"pass\"}'",
    );
    let mut resolution = Resolution::new();
    gated(&mut resolution, &control);
    let git = RealGit::new();
    let grant: [String; 0] = [];
    let window = run_tool_calls(
        rig.ws.path(),
        &rig.worktree,
        "agent-order",
        &resolution.of("sensor", &grant),
        &rig.step_dir_rel,
        &[tool_use("t1", "bash")],
        &rig.deps(&git),
    )
    .unwrap();
    assert!(matches!(window, ToolWindow::Completed));
    assert!(rig.executed().is_empty());
    assert!(!rig.ws.path().join("consulted").exists());
}

#[test]
fn a_control_fault_fails_closed_before_anything_runs() {
    let rig = Rig::new("agent-fault");
    let control = control_script(rig.ws.path(), "echo 'guardian down' >&2; exit 2");
    let mut resolution = Resolution::new();
    gated(&mut resolution, &control);
    let git = RealGit::new();
    let err = rig
        .run("agent-fault", &resolution, &[tool_use("t1", "bash")], &git)
        .unwrap_err();
    let Error::ToolControl { tool, detail, .. } = err else {
        panic!("expected ToolControl, got {err:?}");
    };
    assert_eq!(tool, "bash");
    assert!(detail.contains("exited 2"), "{detail}");
    assert!(rig.executed().is_empty());
    assert!(!rig.worktree.join("messages/002-tool.json").exists());
}

#[test]
fn a_stop_felling_the_control_is_the_stop_not_a_fault() {
    let rig = Rig::new("agent-stopctl");
    let control = control_script(rig.ws.path(), "exec sleep 60");
    let mut resolution = Resolution::new();
    gated(&mut resolution, &control);
    rig.stop.store(true, std::sync::atomic::Ordering::SeqCst);
    let git = RealGit::new();
    let window = rig
        .run(
            "agent-stopctl",
            &resolution,
            &[tool_use("t1", "bash")],
            &git,
        )
        .unwrap();
    assert!(matches!(window, ToolWindow::Stopped));
    assert!(rig.executed().is_empty());
    // Nothing ran, and the window is still settled on the way out (§2.9):
    // the invocation the control never adjudicated is answered in band,
    // so the branch stays revivable ([`super::super::settle`]).
    let settled =
        std::fs::read_to_string(rig.worktree.join("messages/002-tool.json")).expect("settled");
    assert!(settled.contains("\"is_error\":true"), "{settled}");
    assert!(settled.contains("did not return"), "{settled}");
}
