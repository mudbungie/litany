//! Unit tests for the `python` built-in: the program runs and the stub
//! module lands beside the invocation's own record. The refusals are
//! [`super::tests_faults`]'s, split out to keep both files under the
//! repo's per-file line cap; the scene they share is here. The inner
//! invocations a program composes are driven through the real binary in
//! `src/e2e/python_cli.rs` — a door reached from a program is a process
//! boundary, and only a real one proves the model's transcript never
//! sees it.

use super::*;
use crate::harness_root::Roots;
use crate::prompt::step::STEPS_DIR;
use crate::prompt::tool::{ENV_CONV_BRANCH, ENV_CONV_REPO};
use crate::test_support::with_litany_home;
use crate::workspace::fixture;
use serde_json::json;
use std::collections::HashMap;
use std::ffi::OsString;
use std::io::Cursor;
use std::sync::atomic::AtomicBool;
use tempfile::TempDir;

pub(super) const AGENT: &str = "20260101-a1";
pub(super) const TOOL_ID: &str = "tu_prog";

/// The §3.3 contract environment, named without mutating the process's.
pub(super) struct FakeEnv(pub(super) HashMap<&'static str, OsString>);

impl FakeEnv {
    pub(super) fn at(ws: &Path) -> Self {
        Self(HashMap::from([
            (ENV_CONV_REPO, ws.as_os_str().to_owned()),
            (ENV_CONV_BRANCH, OsString::from(AGENT)),
            (ENV_TOOL_ID, OsString::from(TOOL_ID)),
        ]))
    }
}

impl EnvLookup for FakeEnv {
    fn get(&self, key: &str) -> Option<OsString> {
        self.0.get(key).cloned()
    }
}

/// A real workspace with [`AGENT`] forked and its first step present —
/// the in-flight step a program's stub module and inner records land
/// under.
pub(super) struct Scene {
    _holder: TempDir,
    pub(super) home: PathBuf,
    pub(super) ws: PathBuf,
    pub(super) step: PathBuf,
    pub(super) worktree: PathBuf,
}

impl Scene {
    pub(super) fn new() -> Self {
        let holder = TempDir::new().unwrap();
        let home = holder.path().join("home");
        let roots = Roots {
            config: home.clone(),
            data: home.clone(),
        };
        let ws = fixture::workspace_under(&roots);
        let worktree = fixture::spawn_root(&ws, AGENT);
        let step = ws.join(STEPS_DIR).join(AGENT).join("001");
        std::fs::create_dir_all(&step).unwrap();
        Self {
            _holder: holder,
            home,
            ws,
            step,
            worktree,
        }
    }

    /// Run `program` through the built-in, with the adapter target named
    /// so the §4.4 load-time version guard is skipped (a program makes
    /// no model call, so the target is never spawned).
    pub(super) fn run(&self, program: &str) -> (Result<i32, Error>, String, String) {
        self.run_as(&FakeEnv::at(&self.ws), program, INTERPRETER)
    }

    pub(super) fn run_as(
        &self,
        env: &dyn EnvLookup,
        program: &str,
        interpreter: &str,
    ) -> (Result<i32, Error>, String, String) {
        let mut stdout: Vec<u8> = Vec::new();
        let mut stderr: Vec<u8> = Vec::new();
        let input = json!({ "program": program }).to_string();
        let code = self.drive(
            env,
            &mut Cursor::new(input.into_bytes()),
            &mut stdout,
            &mut stderr,
            interpreter,
        );
        (
            code,
            String::from_utf8_lossy(&stdout).into_owned(),
            String::from_utf8_lossy(&stderr).into_owned(),
        )
    }

    pub(super) fn drive<R: Read, W: Write, E: Write>(
        &self,
        env: &dyn EnvLookup,
        stdin: &mut R,
        stdout: &mut W,
        stderr: &mut E,
        interpreter: &str,
    ) -> Result<i32, Error> {
        let adapter = self.ws.join("no-adapter");
        let stop = AtomicBool::new(false);
        let bindings = super::super::Bindings {
            driver_target: Path::new("litany"),
            adapter_target: Some(&adapter),
            stop: &stop,
            injection: None,
        };
        with_litany_home(&self.home, || {
            run_with(stdin, stdout, stderr, &bindings, env, interpreter)
        })
    }

    pub(super) fn record(&self) -> PathBuf {
        self.step.join(STEP_TOOLS_SUBDIR).join(TOOL_ID)
    }
}

#[test]
fn a_program_runs_and_its_streams_come_back_with_the_interpreters_exit_code() {
    let scene = Scene::new();
    let (code, out, err) = scene.run("import sys\nprint('hi')\nsys.stderr.write('note')\n");
    assert_eq!(code.unwrap(), 0, "{out}{err}");
    assert_eq!(out, "hi\n");
    assert_eq!(err, "note");
}

#[test]
fn a_program_that_raises_is_a_non_zero_exit_with_the_traceback_on_stderr() {
    let scene = Scene::new();
    let (code, _, err) = scene.run("raise ValueError('nope')\n");
    assert_eq!(code.unwrap(), 1);
    assert!(err.contains("ValueError: nope"), "{err}");
}

#[test]
fn the_stub_module_lands_beside_the_invocation_s_own_record() {
    let scene = Scene::new();
    let (code, out, err) = scene.run("import litany_tools\nprint(litany_tools.TOOL_ID)\n");
    assert_eq!(code.unwrap(), 0, "{out}{err}");
    // The program imported it, so it was on PYTHONPATH; and it sits in
    // this invocation's own record directory, out of the worktree.
    assert_eq!(out, format!("{TOOL_ID}\n"));
    let module = std::fs::read_to_string(scene.record().join(MODULE)).unwrap();
    assert!(module.contains("\ndef bash(*, command):\n"), "{module}");
    assert!(module.contains("\ndef read_file(*, path):\n"), "{module}");
    // Depth 1: the tool is absent from its own module (ARCH §3.3).
    assert!(!module.contains("\ndef python("), "{module}");
    assert!(
        !scene.worktree.join(MODULE).exists(),
        "the module is diagnostic, never a file in the agent's tree"
    );
}

#[test]
fn the_module_is_regenerated_per_invocation_from_the_grant_as_it_now_reads() {
    // §2.7: the toolset is read at the moment the program runs, so a
    // definition the branch no longer carries is gone from the next
    // program's module rather than snapshotted at fork time.
    let scene = Scene::new();
    std::fs::remove_file(
        scene
            .worktree
            .join("descriptions/tools")
            .join("read_file.json"),
    )
    .unwrap();
    let (code, _, err) = scene.run("print('ok')\n");
    assert_eq!(code.unwrap(), 0, "{err}");
    let module = std::fs::read_to_string(scene.record().join(MODULE)).unwrap();
    assert!(module.contains("\ndef bash(*, command):\n"), "{module}");
    assert!(!module.contains("\ndef read_file("), "{module}");
}

#[test]
fn a_missing_interpreter_is_the_in_band_127_naming_it() {
    let scene = Scene::new();
    let (code, out, err) = scene.run_as(&FakeEnv::at(&scene.ws), "print(1)\n", "no-such-python3");
    assert_eq!(code.unwrap(), NOT_FOUND);
    assert!(out.is_empty(), "{out}");
    assert!(err.contains("no-such-python3: not found"), "{err}");
}

#[test]
fn an_interpreter_that_cannot_be_run_at_all_is_a_harness_fault_not_a_127() {
    // A directory is not a missing binary: the spawn fails for another
    // reason, and the built-in does not dress it as an in-band answer.
    let scene = Scene::new();
    let dir = scene.ws.to_string_lossy().into_owned();
    let (code, _, _) = scene.run_as(&FakeEnv::at(&scene.ws), "print(1)\n", &dir);
    assert!(matches!(code, Err(Error::Child(_))), "{code:?}");
}
