//! Probe / launch / CLI-orchestration tests (ARCH §2.11 *A deposit into
//! a quiescent agent starts a driver*, Writer/driver totality).

use super::super::{
    Launcher, MessageError, ProbeOutcome, USER_SENDER, cli_message, cli_run, inbox_dir,
    probe_and_launch, resolve_cli_sender, try_acquire,
};
use crate::prompt::Clock;
use std::cell::RefCell;
use std::ffi::OsStr;
use std::io;
use std::path::Path;
use tempfile::TempDir;

struct FixedClock;
impl Clock for FixedClock {
    fn now_iso8601(&self) -> String {
        "2026-07-11T00:00:00Z".into()
    }
    fn now_compact(&self) -> String {
        unreachable!("deposit never reads the compact clock")
    }
}

/// Recording [`Launcher`] — captures each launch request.
#[derive(Default)]
struct StubLauncher {
    invocations: RefCell<Vec<String>>,
}
impl Launcher for StubLauncher {
    fn launch(&self, _workspace: &Path, agent_id: &str) -> io::Result<()> {
        self.invocations.borrow_mut().push(agent_id.to_string());
        Ok(())
    }
}

/// [`Launcher`] that fails to spawn — exercises the propagated error.
struct FailLauncher;
impl Launcher for FailLauncher {
    fn launch(&self, _workspace: &Path, _agent_id: &str) -> io::Result<()> {
        Err(io::Error::other("cannot spawn driver"))
    }
}

#[test]
fn probe_launches_a_driver_when_quiescent() {
    let ws = TempDir::new().unwrap();
    let launcher = StubLauncher::default();
    let out = probe_and_launch(ws.path(), "a1", &launcher).unwrap();
    assert_eq!(out, ProbeOutcome::Launched);
    assert_eq!(*launcher.invocations.borrow(), vec!["a1".to_string()]);
}

#[test]
fn probe_is_busy_when_an_executor_holds_the_lock() {
    let ws = TempDir::new().unwrap();
    // Simulate a live executor by holding the lock across the probe.
    let _held = try_acquire(&inbox_dir(ws.path(), "a1"))
        .unwrap()
        .expect("free");
    let launcher = StubLauncher::default();
    let out = probe_and_launch(ws.path(), "a1", &launcher).unwrap();
    assert_eq!(out, ProbeOutcome::Busy);
    assert!(
        launcher.invocations.borrow().is_empty(),
        "no launch while driven"
    );
}

#[test]
fn probe_surfaces_try_acquire_error() {
    let ws = TempDir::new().unwrap();
    std::fs::write(ws.path().join("inbox"), b"not a dir").unwrap();
    let launcher = StubLauncher::default();
    assert!(probe_and_launch(ws.path(), "a1", &launcher).is_err());
}

#[test]
fn probe_propagates_launcher_error() {
    let ws = TempDir::new().unwrap();
    let err = probe_and_launch(ws.path(), "a1", &FailLauncher).unwrap_err();
    assert_eq!(err.to_string(), "cannot spawn driver");
}

#[test]
fn cli_message_deposits_then_launches() {
    let ws = TempDir::new().unwrap();
    let launcher = StubLauncher::default();
    let out = cli_message(ws.path(), "a1", "hello", "user", &FixedClock, &launcher).unwrap();
    assert_eq!(out, ProbeOutcome::Launched);
    assert!(inbox_dir(ws.path(), "a1").join("user-001.md").exists());
    assert_eq!(*launcher.invocations.borrow(), vec!["a1".to_string()]);
}

#[test]
fn cli_message_surfaces_deposit_error() {
    let ws = TempDir::new().unwrap();
    std::fs::write(ws.path().join("inbox"), b"not a dir").unwrap();
    let err = cli_message(
        ws.path(),
        "a1",
        "hi",
        "user",
        &FixedClock,
        &StubLauncher::default(),
    )
    .unwrap_err();
    assert!(matches!(err, MessageError::Deposit(_)), "{err}");
}

#[test]
fn cli_message_surfaces_probe_error() {
    // Deposit succeeds; the launcher fails → MessageError::Probe.
    let ws = TempDir::new().unwrap();
    let err = cli_message(ws.path(), "a1", "hi", "user", &FixedClock, &FailLauncher).unwrap_err();
    assert!(matches!(err, MessageError::Probe(_)), "{err}");
    // The deposit still landed — undelivered, not lost (§2.11).
    assert!(inbox_dir(ws.path(), "a1").join("user-001.md").exists());
}

#[test]
fn resolve_cli_sender_defaults_to_user() {
    assert_eq!(resolve_cli_sender(None), USER_SENDER);
    assert_eq!(resolve_cli_sender(Some(OsStr::new(""))), USER_SENDER);
}

#[test]
fn resolve_cli_sender_uses_branch_when_set() {
    assert_eq!(resolve_cli_sender(Some(OsStr::new("p1-child"))), "p1-child");
}

#[test]
fn cli_run_deposits_via_production_deps() {
    // Exercises the production wiring: SystemClock and the real
    // AdvanceLauncher. The lock is held by the test so the probe
    // observes Busy and no real driver spawns (the launch path is
    // exercised by the launcher tests below and the advance CLI
    // integration test).
    //
    // **The sender is the value passed, never the live environment**
    // (bl-b5b1). It used to be `std::env::var_os` inside `cli_run`, and
    // this beat could only say "a single file landed" because it could
    // not name what the file would be called — the environment is
    // per-process and a sibling beat setting the contract vars for its
    // own run renamed this one's deposit. Both readings are pinned here.
    let (_h, ws) = crate::workspace::fixture::workspace();
    crate::workspace::fixture::spawn_root(&ws, "a1");
    let ws = ws.as_path();
    let _held = try_acquire(&inbox_dir(ws, "a1")).unwrap().expect("free");
    cli_run(ws, "a1", "hi", None, Path::new("true")).unwrap();
    cli_run(
        ws,
        "a1",
        "again",
        Some(OsStr::new("p1-child")),
        Path::new("true"),
    )
    .unwrap();
    let mut files: Vec<String> = std::fs::read_dir(inbox_dir(ws, "a1"))
        .unwrap()
        .flatten()
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| n.ends_with(".md"))
        .collect();
    files.sort();
    assert_eq!(files, ["p1-child-001.md", "user-001.md"], "{files:?}");
}

#[test]
fn cli_run_declines_a_recipient_with_no_branch() {
    // §2.11: a message is addressed to an *existing* agent. A deposit no
    // drain would ever come for is declined loudly — the alternative is
    // silent message loss into a directory nothing reads.
    let (_h, ws) = crate::workspace::fixture::workspace();
    let err = cli_run(ws.as_path(), "a1", "hi", None, Path::new("true")).unwrap_err();
    assert!(
        matches!(err, MessageError::UnknownAgent(_)),
        "an unknown recipient is its own decline: {err}"
    );
    assert!(err.to_string().starts_with("no agent \"a1\""), "{err}");
    assert!(err.to_string().contains("existing agent"), "{err}");
    assert!(
        !inbox_dir(ws.as_path(), "a1").exists(),
        "the decline creates no inbox directory"
    );
}
