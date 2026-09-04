//! `litany invoke`'s refusals — the ones that never reach a gate
//! (ARCH §3.4). A malformed block, an unreadable stdin, a contract var
//! the harness did not set, an agent with no step to record inside, a
//! worktree that is gone, a control that cannot answer, and a stdout
//! that will not take the product: each is a refusal naming its own
//! cause, never a silent empty invocation. The scene is
//! [`super::invoking`]'s.

use super::invoking::{AGENT, FakeEnv, Scene};
use crate::prompt::dispatch::door;
use crate::prompt::tool::{ENV_CONV_BRANCH, ENV_CONV_REPO};
use crate::test_support::{litany_binary, with_litany_home};
use std::collections::HashMap;
use std::io::Write;
use std::sync::atomic::AtomicBool;

#[test]
fn a_malformed_block_is_refused_naming_the_expected_shape() {
    let scene = Scene::new();
    let (code, _) = scene.invoke("{\"id\":\"tu_1\"}");
    let err = code
        .expect_err("a block with no name cannot be run")
        .to_string();
    assert!(err.contains("malformed tool_use block"), "{err}");
    assert!(err.contains("\"name\": \"<tool>\""), "{err}");
}

/// A reader that fails: stdin is a pipe in production, and a read error
/// is a refusal rather than an empty block.
struct FailingRead;

impl std::io::Read for FailingRead {
    fn read(&mut self, _: &mut [u8]) -> std::io::Result<usize> {
        Err(std::io::Error::other("pipe broke"))
    }
}

#[test]
fn an_unreadable_stdin_is_refused_rather_than_read_as_an_empty_block() {
    let scene = Scene::new();
    let mut stdout: Vec<u8> = Vec::new();
    let err = scene
        .drive(&mut FailingRead, &mut stdout, &AtomicBool::new(false))
        .expect_err("an unreadable stdin has no block")
        .to_string();
    assert!(err.contains("read the tool_use block from stdin"), "{err}");
}

/// A writer that fails, so the door's own product has a failure path.
struct FailingWrite;

impl std::io::Write for FailingWrite {
    fn write(&mut self, _: &[u8]) -> std::io::Result<usize> {
        Err(std::io::Error::other("stdout closed"))
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

#[test]
fn an_unwritable_stdout_is_surfaced_rather_than_dropped() {
    let scene = Scene::new();
    let mut stdin = r#"{"id":"tu_1","name":"nosuch","input":{}}"#.as_bytes();
    let err = scene
        .drive(&mut stdin, &mut FailingWrite, &AtomicBool::new(false))
        .expect_err("the product could not be written")
        .to_string();
    assert!(err.contains("write the result envelope"), "{err}");
    // The door writes its product and never flushes it — the only
    // failure it can see is the write, and this says so.
    assert!(FailingWrite.flush().is_ok());
}

/// Both contract vars are required, and a non-UTF-8 branch is the same
/// decline as an absent one — the harness sets them (§3.3), so an
/// invocation without them was not raised by a step.
#[test]
fn a_missing_contract_var_is_declined_naming_it() {
    let scene = Scene::new();
    let block = r#"{"id":"tu_1","name":"bash","input":{}}"#;
    for (key, env) in [
        (ENV_CONV_REPO, FakeEnv(HashMap::new())),
        (
            ENV_CONV_BRANCH,
            FakeEnv(HashMap::from([(
                ENV_CONV_REPO,
                scene.ws.as_os_str().to_owned(),
            )])),
        ),
        (ENV_CONV_BRANCH, {
            let mut env = FakeEnv::at(&scene.ws);
            env.0.insert(
                ENV_CONV_BRANCH,
                std::os::unix::ffi::OsStringExt::from_vec(vec![0xff, 0xfe]),
            );
            env
        }),
    ] {
        let mut stdin = block.as_bytes();
        let mut stdout: Vec<u8> = Vec::new();
        let adapter = scene.ws.join("no-adapter");
        let err = with_litany_home(&scene.home, || {
            door::cli::run(
                &env,
                &mut stdin,
                &mut stdout,
                &litany_binary(),
                Some(&adapter),
                &AtomicBool::new(false),
                None,
            )
        })
        .expect_err("the contract var is missing")
        .to_string();
        assert!(err.contains(key), "{err}");
    }
}

#[test]
fn an_agent_with_no_step_has_nowhere_to_record_and_is_refused() {
    let scene = Scene::new();
    std::fs::remove_dir_all(&scene.step).unwrap();
    let (code, _) = scene.invoke(r#"{"id":"tu_1","name":"bash","input":{}}"#);
    let err = code.expect_err("no step, no record").to_string();
    assert!(err.contains("has no step under"), "{err}");
}

#[test]
fn a_worktree_that_is_gone_is_the_executor_s_refusal_not_a_silent_run() {
    // The executor derives the calling agent's worktree from the step
    // dir and declines rather than running a tool in an inherited cwd
    // (ARCH §3.3 *Working directory*). The door surfaces that verbatim.
    let scene = Scene::new();
    std::fs::remove_dir_all(crate::workspace::agent_worktree(&scene.ws, AGENT)).unwrap();
    let (code, _) = scene.invoke(r#"{"id":"tu_w","name":"bash","input":{"command":"echo hi"}}"#);
    let err = code.expect_err("no worktree, no tool").to_string();
    assert!(err.contains("worktree"), "{err}");
}

#[test]
fn a_control_that_cannot_answer_fails_closed() {
    // §3.3 *Tool control*: a control fault is never a pass. The door
    // carries the window's own error out rather than running the tool.
    let scene = Scene::new();
    super::invoking_gates::control(&scene, "exit 1");
    let (code, out) = scene.invoke(r#"{"id":"tu_c","name":"bash","input":{"command":"echo ran"}}"#);
    let err = code
        .expect_err("a faulting control is not a pass")
        .to_string();
    assert!(err.contains("tool control"), "{err}{out}");
    assert!(
        !scene.recorded("tu_c"),
        "nothing ran behind a faulting control"
    );
}
