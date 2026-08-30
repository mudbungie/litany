//! End-to-end integration of the `bash` built-in (ARCH §3.3, §12 v0.3
//! toolset) through the tool executor.
//!
//! Mirrors `tests/read_file_tool.rs` but exercises the bash surface:
//!
//! 1. Stdout bytes from the spawned shell are surfaced verbatim — the
//!    §3.3 stdio contract.
//! 2. `is_error` is `false` on a zero-exit command.
//! 3. The per-tool-call disk record lands at `<step>/tools/<tool-id>/`
//!    with `input.json` and `output.json` per §3.3 "Disk record".
//! 4. A failure mode (`false`) round-trips: `is_error: true`, the exit
//!    code stated and stderr under its marker in `tool_result.content`,
//!    `output.json.exit_code != 0`.

use super::fixtures::{StepDir, after_header};
use crate::prompt::clock::SystemClock;
use crate::prompt::tool::spawn::PathLookup;
use crate::prompt::tool::{
    INPUT_FILE, OUTPUT_FILE, STEP_TOOLS_SUBDIR, SpawnTool, ToolCall, ToolExecutor, ToolInputRecord,
    ToolOutputRecord,
};
use serde_json::json;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use tempfile::TempDir;

fn litany_bin() -> PathBuf {
    crate::test_support::litany_binary()
}

/// Forces the §3.3 second hop to miss, so resolution falls through to
/// the injected driver target — here the cargo-built `litany` binary.
/// The harness root is kept empty (the first hop misses too), and PATH
/// is short-circuited here so the test never depends on the live env.
struct NoPath;

impl PathLookup for NoPath {
    fn which_on_path(&self, _prefixed_name: &str) -> Option<PathBuf> {
        None
    }
}

struct Fixture {
    _harness_root: TempDir,
    step: StepDir,
}

impl Fixture {
    fn new() -> Self {
        let harness = TempDir::new().expect("harness root tempdir");
        std::fs::create_dir_all(harness.path().join("tools")).unwrap();
        Self {
            _harness_root: harness,
            step: StepDir::new(),
        }
    }

    fn harness_path(&self) -> &std::path::Path {
        self._harness_root.path()
    }
}

fn executor<'a>(harness: &'a Path, clock: &'a SystemClock, litany: &'a Path) -> SpawnTool<'a> {
    SpawnTool::new(harness, clock, litany).with_path_lookup(Box::new(NoPath))
}

#[test]
fn bash_through_executor_returns_stdout_and_lands_disk_record() {
    let fixture = Fixture::new();

    let clock = SystemClock;
    let litany = litany_bin();
    let exec = executor(fixture.harness_path(), &clock, &litany);
    let outcome = exec
        .execute(
            ToolCall {
                id: "toolu_bash_ok",
                name: "bash",
                input: &json!({ "command": "printf hello-from-bash" }),
            },
            &fixture.step.path,
            &AtomicBool::new(false),
            None,
        )
        .expect("execute succeeds");

    assert!(!outcome.is_error, "happy-path is_error should be false");
    assert_eq!(outcome.content, b"Exit code: 0\nhello-from-bash");

    let dir = fixture
        .step
        .path
        .join(STEP_TOOLS_SUBDIR)
        .join("toolu_bash_ok");
    let input: ToolInputRecord =
        serde_json::from_slice(&std::fs::read(dir.join(INPUT_FILE)).unwrap()).unwrap();
    assert_eq!(input.id, "toolu_bash_ok");
    assert_eq!(input.name, "bash");
    assert_eq!(input.input["command"], json!("printf hello-from-bash"));

    let output: ToolOutputRecord =
        serde_json::from_slice(&std::fs::read(dir.join(OUTPUT_FILE)).unwrap()).unwrap();
    assert_eq!(output.exit_code, 0);
    assert_eq!(output.stdout, "hello-from-bash");
    assert_eq!(output.stderr, "");
    assert!(!output.started_at.is_empty(), "started_at present");
    assert!(!output.ended_at.is_empty(), "ended_at present");
}

#[test]
fn bash_writes_land_in_the_agents_worktree_not_the_launchers_cwd() {
    // The §3.3 *Working directory* contract, end to end through the real
    // `litany tool bash` re-entry: the shell the built-in forks inherits
    // the cwd the executor pinned, so `> out.txt` lands on the agent's
    // branch. Before the pin it landed in whatever directory the harness
    // process was launched from — the operator's shell (bl-2503).
    let fixture = Fixture::new();
    let launcher_cwd = std::env::current_dir().expect("cwd");

    let clock = SystemClock;
    let litany = litany_bin();
    let exec = executor(fixture.harness_path(), &clock, &litany);
    let outcome = exec
        .execute(
            ToolCall {
                id: "toolu_bash_cwd",
                name: "bash",
                input: &json!({ "command": "echo hello > out.txt; pwd" }),
            },
            &fixture.step.path,
            &AtomicBool::new(false),
            None,
        )
        .expect("execute succeeds");

    assert!(!outcome.is_error, "write should succeed: {outcome:?}");
    let reported = String::from_utf8_lossy(after_header(&outcome.content))
        .trim()
        .to_string();
    assert_eq!(
        std::fs::canonicalize(reported).unwrap(),
        std::fs::canonicalize(&fixture.step.worktree).unwrap(),
        "the shell runs in the agent's worktree",
    );
    assert_eq!(
        std::fs::read_to_string(fixture.step.worktree.join("out.txt")).unwrap(),
        "hello\n",
    );
    assert!(
        !launcher_cwd.join("out.txt").exists(),
        "nothing may land in the launcher's cwd {launcher_cwd:?}",
    );
}

#[test]
fn bash_failure_states_its_exit_code_and_marks_stderr() {
    let fixture = Fixture::new();

    let clock = SystemClock;
    let litany = litany_bin();
    let exec = executor(fixture.harness_path(), &clock, &litany);
    let outcome = exec
        .execute(
            ToolCall {
                id: "toolu_bash_err",
                name: "bash",
                input: &json!({
                    "command": "printf prelude; printf complaint 1>&2; exit 7"
                }),
            },
            &fixture.step.path,
            &AtomicBool::new(false),
            None,
        )
        .expect("execute returns Ok even when the shell exits non-zero");

    assert!(outcome.is_error, "failure path: is_error must be true");
    let content = String::from_utf8_lossy(&outcome.content);
    assert!(
        content.contains("prelude"),
        "stdout fragment missing: {content:?}",
    );
    assert!(
        content.contains("complaint"),
        "stderr fragment should follow stdout under its marker: {content:?}",
    );

    let dir = fixture
        .step
        .path
        .join(STEP_TOOLS_SUBDIR)
        .join("toolu_bash_err");
    let output: ToolOutputRecord =
        serde_json::from_slice(&std::fs::read(dir.join(OUTPUT_FILE)).unwrap()).unwrap();
    assert_eq!(output.exit_code, 7);
    assert_eq!(output.stdout, "prelude");
    assert_eq!(output.stderr, "complaint");
}
