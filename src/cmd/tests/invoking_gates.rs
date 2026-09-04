//! The door's gates, driven through `litany invoke` (ARCH §3.3,
//! `docs/DESIGN_CODE_EXECUTION.md` §2.1): the depth refusal, the grant
//! gate, the tool control's three verdicts, and the §2.9 stop. Every
//! one of them settles the invocation *before* the executor, so the
//! shared assertion is that no record landed. The scene and the driving
//! helpers are [`super::invoking`]'s.

use super::invoking::{AGENT, Scene};
use crate::workspace::fixture;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::AtomicBool;

/// A `workflow.yaml` whose tool control is `command`.
fn workflow_with_control(command: &Path) -> String {
    format!(
        "events: {{}}\ntool_control:\n  command: {}\n",
        command.display()
    )
}

/// Lay down an executable control script beside the workspace and put
/// it in the governing config's `tool_control:` (ARCH §3.3).
///
/// The script is written **by a child**, never by this process: an
/// `fs::write` here holds a write fd, a fork on any other thread copies
/// it into a child that keeps it until its own exec, and an exec of the
/// script inside that window is `ETXTBSY`. The suite forks `git`
/// constantly, so the window is real; `sh -c 'cat > … && chmod'` leaves
/// no descriptor in this process for any fork to copy.
pub(super) fn control(scene: &Scene, body: &str) -> PathBuf {
    let path = scene.ws.parent().unwrap_or(&scene.ws).join("control");
    write_exec(&path, &format!("#!/usr/bin/env bash\n{body}\n"));
    fixture::amend_config(
        &scene.ws,
        &[("workflow.yaml", &workflow_with_control(&path))],
    );
    path
}

/// Write `body` to `path` and make it executable, from a child process
/// ([`control`] says why).
fn write_exec(path: &Path, body: &str) {
    let mut child = Command::new("sh")
        .arg("-c")
        .arg(r#"cat > "$1" && chmod 755 "$1""#)
        .arg("sh")
        .arg(path)
        .stdin(Stdio::piped())
        .spawn()
        .expect("spawn the writer");
    child
        .stdin
        .take()
        .expect("stdin is piped")
        .write_all(body.as_bytes())
        .expect("write the script");
    assert!(child.wait().expect("reap the writer").success());
}

/// Depth 1: a tool that composes invocations of its own may not be one
/// — and the refusal precedes the grant gate, so it holds for `python`,
/// which the worker *is* granted. A retired composer (`multi_tool`) is
/// no longer a depth case at all: it is an ungranted name like any
/// other, declined by the gate below.
#[test]
fn a_composing_tool_is_refused_at_depth_one() {
    let scene = Scene::new();
    let (code, out) = scene.invoke(r#"{"id":"tu_d","name":"python","input":{}}"#);
    assert!(matches!(code, Ok(1)), "{out}");
    assert!(
        out.contains("composes tool invocations of its own and may not be one (depth 1)"),
        "{out}"
    );
    assert!(
        !scene.recorded("tu_d"),
        "a refused invocation records nothing"
    );
}

/// The retired multi-tool is an ordinary ungranted name (`docs/
/// DESIGN_CODE_EXECUTION.md` §5): no schema ships, no role grants it,
/// and a model that names it — because its own inherited transcript
/// does — is declined in band rather than answered.
#[test]
fn a_retired_multi_tool_is_declined_as_any_ungranted_name() {
    let scene = Scene::new();
    let (code, out) = scene.invoke(r#"{"id":"tu_m","name":"multi_tool","input":{}}"#);
    assert!(matches!(code, Ok(1)), "{out}");
    assert!(out.contains("is not callable by a worker"), "{out}");
    assert!(!scene.recorded("tu_m"));
}

/// The grant gate's own voice, unchanged — the door reaches the same
/// function the tool window does (§3.3 *declaring is not permitting*).
#[test]
fn a_tool_outside_the_role_grant_is_declined_in_the_grant_gate_s_voice() {
    let scene = Scene::new();
    let (code, out) = scene.invoke(r#"{"id":"tu_g","name":"nosuch","input":{}}"#);
    assert!(matches!(code, Ok(1)), "{out}");
    assert!(out.contains("is not callable by a worker"), "{out}");
    assert!(out.contains("declaring is not permitting"), "{out}");
    assert!(!scene.recorded("tu_g"));
}

#[test]
fn a_control_refusal_settles_the_invocation_before_the_executor() {
    let scene = Scene::new();
    control(
        &scene,
        r#"echo '{"verdict":"refuse","reason":"not this one"}'"#,
    );
    let (code, out) = scene.invoke(r#"{"id":"tu_r","name":"bash","input":{"command":"echo ran"}}"#);
    assert!(matches!(code, Ok(1)), "{out}");
    assert!(
        out.contains("was refused by the workflow's tool control"),
        "{out}"
    );
    assert!(out.contains("not this one"), "{out}");
    assert!(!scene.recorded("tu_r"));
}

/// A hold cannot park an invocation another tool is composing: entries
/// (or statements) before it have already run, so it degrades to the
/// in-band decline that names where a hold *can* park.
#[test]
fn a_control_hold_degrades_to_the_re_issue_top_level_decline() {
    let scene = Scene::new();
    control(
        &scene,
        r#"echo '{"verdict":"hold","reason":"needs review"}'"#,
    );
    let (code, out) = scene.invoke(r#"{"id":"tu_h","name":"bash","input":{"command":"echo ran"}}"#);
    assert!(matches!(code, Ok(1)), "{out}");
    assert!(
        out.contains("cannot park an invocation another tool is composing"),
        "{out}"
    );
    assert!(
        out.contains("re-issue this invocation as a top-level tool_use"),
        "{out}"
    );
    assert!(out.contains("needs review"), "{out}");
    assert!(!scene.recorded("tu_h"));
    // The hold mark is the top-level window's act; the door writes none.
    assert!(
        crate::workspace::hold::read(&scene.ws, AGENT, &crate::template::RealGit::new()).is_none(),
        "a degraded hold writes no mark"
    );
}

/// The stop cascade felling the control mid-consult is the stop, not a
/// fault — the same classification the tool window makes (§2.9).
#[test]
fn the_stop_landing_on_the_control_ceases_the_invocation() {
    let scene = Scene::new();
    control(&scene, "kill -TERM $$");
    let stop = AtomicBool::new(true);
    let (code, out) = scene.invoke_with(
        r#"{"id":"tu_s","name":"bash","input":{"command":"echo ran"}}"#,
        &stop,
    );
    assert!(matches!(code, Ok(1)), "{out}");
    assert!(out.contains("the harness is stopping"), "{out}");
    assert!(!scene.recorded("tu_s"));
}
