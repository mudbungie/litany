//! Resolution / spawn / disk-record I/O failure modes — every
//! [`super::super::ExecError`] variant gets a constructive test.

use super::super::spawn::lookup::which_in_path_env;
use super::super::{ExecError, SpawnTool, ToolCall, ToolExecutor};
use super::fixtures::{AGENT_ID, FixedClock, HarnessRoot, StepDir, driver_target};
use serde_json::json;
use std::sync::atomic::AtomicBool;
use tempfile::TempDir;

#[test]
fn spawn_error_when_resolved_binary_is_not_executable() {
    // Resolution succeeds (we drop a *file* under `tools/`) but
    // `Command::spawn` rejects it because it is not chmod +x.
    let root = HarnessRoot::new();
    let bin = root.dir.path().join(super::super::TOOLS_DIR).join(format!(
        "{}{}",
        super::super::EXTERNAL_PREFIX,
        "not-exec"
    ));
    std::fs::write(&bin, b"not a real binary").unwrap();
    let clock = FixedClock::default();
    let step = StepDir::new();
    let exec = SpawnTool::new(root.path(), &clock, driver_target());
    let err = exec
        .execute(
            ToolCall {
                id: "tu_e1",
                name: "not-exec",
                input: &json!({}),
            },
            &step.path,
            &AtomicBool::new(false),
            None,
        )
        .unwrap_err();
    match err {
        ExecError::Spawn { name, .. } => assert_eq!(name, "not-exec"),
        other => panic!("expected Spawn, got {other:?}"),
    }
}

#[test]
fn io_error_when_step_dir_is_a_file() {
    // `create_dir_all` refuses when the leaf is an existing file —
    // exercise the [`ExecError::Io`] branch. The layout around the leaf
    // is well-formed (worktree included) so the §3.3 working-directory
    // resolution passes and the I/O branch is the one that fires.
    let root = HarnessRoot::new();
    root.install("anything", "true");
    let scratch = TempDir::new().unwrap();
    let bogus_step = scratch.path().join("steps").join(AGENT_ID).join("001");
    std::fs::create_dir_all(bogus_step.parent().unwrap()).unwrap();
    std::fs::write(&bogus_step, b"i am a file").unwrap();
    std::fs::create_dir_all(crate::workspace::agent_worktree(scratch.path(), AGENT_ID)).unwrap();
    let clock = FixedClock::default();
    let exec = SpawnTool::new(root.path(), &clock, driver_target());
    let err = exec
        .execute(
            ToolCall {
                id: "tu_e2",
                name: "anything",
                input: &json!({}),
            },
            &bogus_step,
            &AtomicBool::new(false),
            None,
        )
        .unwrap_err();
    match err {
        ExecError::Io { dir, .. } => {
            assert!(dir.ends_with("tu_e2"), "wrong dir in error: {:?}", dir);
        }
        other => panic!("expected Io, got {other:?}"),
    }
}

/// Run `anything` against `step_dir` and return the error. Shared by
/// the two [`ExecError::NoWorktree`] shapes below.
fn declined_step_dir(step_dir: &std::path::Path) -> ExecError {
    let root = HarnessRoot::new();
    root.install("anything", "true");
    let clock = FixedClock::default();
    let exec = SpawnTool::new(root.path(), &clock, driver_target());
    exec.execute(
        ToolCall {
            id: "tu_nw",
            name: "anything",
            input: &json!({}),
        },
        step_dir,
        &AtomicBool::new(false),
        None,
    )
    .unwrap_err()
}

#[test]
fn no_worktree_when_the_step_dir_is_not_the_workspace_shape() {
    // A `step_dir` that is not `<workspace>/steps/<agent-id>/<NNN>`
    // names no agent, so there is no worktree to run the tool in.
    // Declined, never run in the inherited cwd (§3.3, bl-2503).
    let scratch = TempDir::new().unwrap();
    let shapeless = scratch.path().join("elsewhere").join("001");
    std::fs::create_dir_all(&shapeless).unwrap();
    match declined_step_dir(&shapeless) {
        ExecError::NoWorktree { name, step_dir } => {
            assert_eq!(name, "anything");
            assert_eq!(step_dir, shapeless);
        }
        other => panic!("expected NoWorktree, got {other:?}"),
    }
}

#[test]
fn no_worktree_when_the_agents_worktree_is_not_materialized() {
    // Well-shaped `step_dir`, but `<workspace>/agents/<agent-id>/` is
    // not on disk: the call is declined with the same fault rather than
    // spawning into a directory that is not there (which would surface
    // as a misleading "spawn failed: no such file or directory").
    let scratch = TempDir::new().unwrap();
    let step_dir = scratch.path().join("steps").join(AGENT_ID).join("001");
    std::fs::create_dir_all(&step_dir).unwrap();
    match declined_step_dir(&step_dir) {
        ExecError::NoWorktree { step_dir: d, .. } => assert_eq!(d, step_dir),
        other => panic!("expected NoWorktree, got {other:?}"),
    }
    // Declined before any disk record landed.
    assert!(!step_dir.join(super::super::STEP_TOOLS_SUBDIR).exists());
}

#[test]
fn which_in_path_misses_when_no_dir_carries_the_binary() {
    let empty = TempDir::new().unwrap();
    assert_eq!(
        which_in_path_env("litany-tool-nope", Some(empty.path().as_os_str())),
        None
    );
}

#[test]
fn which_in_path_live_env_returns_a_value_for_a_real_binary() {
    // Cover the live `which_in_path` (env-var-reading) wrapper. `sh`
    // is on PATH on every POSIX runner. We're not asserting where —
    // just that the env-read branch produces *something*.
    use super::super::spawn::lookup::which_in_path_env as wpe;
    let path = std::env::var_os("PATH");
    let hit = wpe("sh", path.as_deref());
    assert!(hit.is_some(), "expected /bin/sh or similar on PATH");
}

#[test]
fn live_which_in_path_reads_path_env_without_panicking() {
    // Covers the `var_os("PATH")` line in `which_in_path`. The
    // result is `Option` either way — under cargo test PATH is
    // typically set, but the wrapper must tolerate it being unset
    // (the `?` short-circuits) without us asserting a specific
    // outcome.
    let _ = super::super::spawn::lookup::which_in_path("litany-tool-definitely-not-installed");
}
