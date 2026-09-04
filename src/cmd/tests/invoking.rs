//! `litany invoke` — the door verb (ARCH §3.4,
//! `docs/DESIGN_CODE_EXECUTION.md` §2.1). This file holds the scene the
//! other two share ([`Scene`]) and the invocations that run:
//! [`super::invoking_gates`] drives the gates that settle one before the
//! executor, and [`super::invoking_faults`] the refusals that never
//! reach a gate at all.

use crate::harness_root::Roots;
use crate::prompt::dispatch::door;
use crate::prompt::step::STEPS_DIR;
use crate::prompt::tool::{ENV_CONV_BRANCH, ENV_CONV_REPO, STEP_TOOLS_SUBDIR};
use crate::test_support::{litany_binary, with_contract_env, with_litany_home};
use crate::workspace::fixture;
use std::collections::HashMap;
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use tempfile::TempDir;

/// The agent every scene raises its invocation on behalf of.
pub(super) const AGENT: &str = "20260101-a1";

/// A [`door::cli::run`]-shaped environment lookup, so a test names the
/// §3.3 contract vars without mutating the process's own environment.
pub(super) struct FakeEnv(pub(super) HashMap<&'static str, OsString>);

impl FakeEnv {
    /// Both contract vars, pointing at `ws` and [`AGENT`].
    pub(super) fn at(ws: &Path) -> Self {
        Self(HashMap::from([
            (ENV_CONV_REPO, ws.as_os_str().to_owned()),
            (ENV_CONV_BRANCH, OsString::from(AGENT)),
        ]))
    }
}

impl crate::prompt::tool::builtin::dispatch::EnvLookup for FakeEnv {
    fn get(&self, key: &str) -> Option<OsString> {
        self.0.get(key).cloned()
    }
}

/// A real workspace whose harness root is one directory (the shape
/// `LITANY_HOME` resolves to), with [`AGENT`] forked and its first step
/// directory present — the in-flight step an inner invocation records
/// inside.
pub(super) struct Scene {
    _holder: TempDir,
    pub(super) home: PathBuf,
    pub(super) ws: PathBuf,
    pub(super) step: PathBuf,
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
        fixture::spawn_root(&ws, AGENT);
        let step = ws.join(STEPS_DIR).join(AGENT).join("001");
        std::fs::create_dir_all(&step).unwrap();
        Self {
            _holder: holder,
            home,
            ws,
            step,
        }
    }

    /// Drive the door over `block`, with the adapter target named so the
    /// §4.4 load-time version guard is skipped (no model call happens in
    /// a door invocation, so the target is never spawned).
    pub(super) fn invoke(&self, block: &str) -> (Result<i32, door::cli::Error>, String) {
        self.invoke_with(block, &AtomicBool::new(false))
    }

    pub(super) fn invoke_with(
        &self,
        block: &str,
        stop: &AtomicBool,
    ) -> (Result<i32, door::cli::Error>, String) {
        let mut stdout: Vec<u8> = Vec::new();
        let mut stdin = block.as_bytes();
        let code = self.drive(&mut stdin, &mut stdout, stop);
        (code, String::from_utf8_lossy(&stdout).into_owned())
    }

    /// The one call into the door, over this scene's own harness root:
    /// the verb resolves it from the ambient `LITANY_HOME` (§2.2), so a
    /// test that did not set it would read the operator's real one.
    pub(super) fn drive(
        &self,
        stdin: &mut dyn std::io::Read,
        stdout: &mut dyn std::io::Write,
        stop: &AtomicBool,
    ) -> Result<i32, door::cli::Error> {
        let adapter = self.ws.join("no-adapter");
        with_litany_home(&self.home, || {
            door::cli::run(
                &FakeEnv::at(&self.ws),
                stdin,
                stdout,
                &litany_binary(),
                Some(&adapter),
                stop,
                None,
            )
        })
    }

    /// Whether a tool-call record landed under the in-flight step.
    pub(super) fn recorded(&self, id: &str) -> bool {
        self.step.join(STEP_TOOLS_SUBDIR).join(id).exists()
    }
}

/// A tool the worker grant carries, exercised through the §3.3 third
/// hop (`<driver target> tool bash`).
#[test]
fn a_permitted_invocation_runs_and_answers_with_the_raw_envelope() {
    let scene = Scene::new();
    let (code, out) = scene.invoke(
        r#"{"id":"tu_1","name":"bash","input":{"command":"echo hi; echo warn >&2; exit 3"}}"#,
    );
    assert_eq!(code.map_err(|e| e.to_string()), Ok(3), "{out}");
    assert!(out.starts_with("Exit code: 3\nhi\n"), "{out}");
    assert!(out.contains("--- stderr ---\nwarn\n"), "{out}");
    // The record landed under the id the caller minted, in the
    // in-flight step (§2.3) — and nothing was committed for it.
    assert!(scene.recorded("tu_1"));
    let head = crate::template::GitRunner::run_capture(
        &crate::template::RealGit::new(),
        &crate::workspace::agent_worktree(&scene.ws, AGENT),
        &["log", "--format=%s"],
    )
    .unwrap();
    assert_eq!(head.lines().next(), Some("dispatch"), "{head}");
}

/// An omitted `input` is the empty object — the general path with the
/// field absent, not a special case.
#[test]
fn an_omitted_input_is_the_empty_object() {
    let scene = Scene::new();
    let (code, out) = scene.invoke(r#"{"id":"tu_2","name":"bash"}"#);
    // `bash` declines an input with no `command`; the point is that the
    // block parsed and reached the tool at all.
    assert!(matches!(code, Ok(c) if c != 0), "{out}");
    assert!(scene.recorded("tu_2"));
}

/// The verb itself, over the real process environment — the same run,
/// reached the way a composing tool reaches it.
#[test]
fn the_verb_reads_the_contract_from_the_process_environment() {
    let scene = Scene::new();
    let block = r#"{"id":"tu_v","name":"bash","input":{"command":"echo through"}}"#;
    let (outcome, stdout, _) = with_contract_env(&scene.home, &scene.ws, AGENT, || {
        super::with_fx(
            &litany_binary().to_string_lossy(),
            block.as_bytes(),
            &super::noop_editor,
            |fx| {
                // Name the adapter target so the §4.4 load-time version
                // guard is skipped: a door invocation makes no model
                // call, so the target is never spawned and the verdict
                // never depends on which `bz` this box has installed.
                fx.adapter_target = Some(scene.ws.join("no-adapter"));
                super::Command::Invoke(super::invoke::Args {}).run(fx)
            },
        )
    });
    assert!(
        matches!(outcome, Ok(super::Outcome::Code(0))),
        "{outcome:?}"
    );
    assert_eq!(String::from_utf8_lossy(&stdout), "Exit code: 0\nthrough\n");
}

#[test]
fn the_verb_carries_its_own_failure_prefix() {
    // No contract vars in this process's own environment.
    let (outcome, ..) = super::with_fx("litany", b"{}", &super::noop_editor, |fx| {
        super::invoke::run(super::invoke::Args {}, fx)
    });
    super::assert_prefixed(outcome.expect_err("no block, no invocation"), "invoke");
}
