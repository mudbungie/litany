//! Probe / launch / CLI-orchestration tests (ARCH §2.11 *A deposit into
//! a quiescent agent starts a driver*, Writer/driver totality).

use super::super::{
    AdvanceLauncher, Launcher, MessageError, ProbeOutcome, USER_SENDER, cli_message, cli_run,
    inbox_dir, probe_and_launch, resolve_cli_sender, try_acquire,
};
use crate::prompt::Clock;
use std::cell::RefCell;
use std::ffi::OsStr;
use std::io;
use std::os::unix::fs::PermissionsExt;
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
    // Exercises the production wiring: env-derived sender, SystemClock,
    // the real AdvanceLauncher. Whatever `LITANY_CONV_BRANCH` is in the
    // test env, a single message file must land. The lock is held by
    // the test so the probe observes Busy and no real driver spawns
    // (the launch path is exercised by the launcher tests below and the
    // advance CLI integration test).
    let (_h, ws) = crate::workspace::fixture::workspace();
    crate::workspace::fixture::spawn_root(&ws, "a1");
    let ws = ws.as_path();
    let _held = try_acquire(&inbox_dir(ws, "a1")).unwrap().expect("free");
    cli_run(ws, "a1", "hi", Path::new("true")).unwrap();
    let files: Vec<_> = std::fs::read_dir(inbox_dir(ws, "a1"))
        .unwrap()
        .flatten()
        .filter(|e| e.file_name().to_string_lossy().ends_with(".md"))
        .collect();
    assert_eq!(files.len(), 1, "exactly one deposit landed");
}

#[test]
fn cli_run_declines_a_recipient_with_no_branch() {
    // §2.11: a message is addressed to an *existing* agent. A deposit no
    // drain would ever come for is declined loudly — the alternative is
    // silent message loss into a directory nothing reads.
    let (_h, ws) = crate::workspace::fixture::workspace();
    let err = cli_run(ws.as_path(), "a1", "hi", Path::new("true")).unwrap_err();
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

#[test]
fn advance_launcher_spawns_detached_and_returns_at_once() {
    // Fire-and-forget (§2.11): `true` accepts the advance args and exits
    // 0; launch returns as soon as the spawn lands, never waiting. The
    // sink is opened before the spawn, so it exists on return even
    // though the child is never waited on.
    let ws = TempDir::new().unwrap();
    let launcher = AdvanceLauncher::with_exe("true".into());
    launcher.launch(ws.path(), "a1").unwrap();
    assert!(
        driver_log_path(ws.path(), "a1").is_file(),
        "the launch opens the driver's stderr sink under steps/ (§2.11)"
    );
}

/// The §2.11 stderr capture: what a detached driver writes to stderr
/// lands in `steps/<agent-id>/driver.log` instead of `/dev/null`, and a
/// second launch **appends** rather than truncating the first's record.
/// A stub script stands in for `litany advance` — under test is the fd
/// the launcher binds, not what the driver chooses to say through it.
#[test]
fn advance_launcher_captures_child_stderr_and_appends_across_launches() {
    let ws = TempDir::new().unwrap();
    let exe = ws.path().join("stub-driver");
    std::fs::write(&exe, "#!/bin/sh\necho declined >&2\n").unwrap();
    std::fs::set_permissions(&exe, std::fs::Permissions::from_mode(0o755)).unwrap();
    let launcher = AdvanceLauncher::with_exe(exe);

    launcher.launch(ws.path(), "a1").unwrap();
    assert_eq!(
        declines_until(ws.path(), "a1", 1),
        1,
        "the first is captured"
    );
    launcher.launch(ws.path(), "a1").unwrap();
    assert_eq!(
        declines_until(ws.path(), "a1", 2),
        2,
        "the second appends; nothing truncates the first"
    );
}

/// Count `declined` lines in the driver log, retrying while fewer than
/// `want` are visible — the child is fire-and-forget (§2.11), so its
/// write lands after the launch returns. The budget is a *count*, never
/// a wall-clock deadline (§2.9: "a deadline measured under load reports
/// the load"), and it is injected below so both arms are exercisable.
fn declines_until(workspace: &Path, agent_id: &str, want: usize) -> usize {
    declines_within(workspace, agent_id, want, 600)
}

/// [`declines_until`] with the retry budget injected — the shape the
/// sibling lock poll (`crate::prompt::tests::advance::free_within`)
/// already uses, and for the same reason. A count bounds how long the
/// poll waits, not which arms run: when the child has already written
/// by the first read the loop breaks on its first pass, the sleep never
/// executes, and the 100% floor reports one uncovered line on a tree
/// that passed minutes earlier (the bl-2625 flake, bl-1c2e before it).
/// A `want` the log can never reach spends the whole budget instead.
fn declines_within(workspace: &Path, agent_id: &str, want: usize, retries: u32) -> usize {
    let path = driver_log_path(workspace, agent_id);
    let mut seen = 0;
    for attempt in 0..retries {
        if attempt > 0 {
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        seen = std::fs::read_to_string(&path)
            .map(|s| s.matches("declined").count())
            .unwrap_or(0);
        if seen >= want {
            break;
        }
    }
    seen
}

#[test]
fn declines_within_gives_up_when_the_count_never_arrives() {
    // No launch, so no driver log and no `declined` line will ever
    // appear: the poll spends its whole budget on every box, which is
    // what makes the retry sleep and the give-up return deterministic.
    let ws = TempDir::new().unwrap();
    assert_eq!(declines_within(ws.path(), "a1", 1, 2), 0);
}

/// The launch is **declined** when its stderr sink cannot be opened
/// (PRINCIPLES *Decline illegal operations*) — no silent fallback to
/// null. A regular file where `steps/` must be a directory is the
/// smallest unwritable workspace.
#[test]
fn advance_launcher_declines_when_the_stderr_sink_cannot_be_opened() {
    let ws = TempDir::new().unwrap();
    std::fs::write(ws.path().join(crate::prompt::step::STEPS_DIR), b"not a dir").unwrap();
    let launcher = AdvanceLauncher::with_exe("true".into());
    let err = launcher.launch(ws.path(), "a1").unwrap_err();
    assert_eq!(err.kind(), io::ErrorKind::NotADirectory);
}

fn driver_log_path(workspace: &Path, agent_id: &str) -> std::path::PathBuf {
    workspace
        .join(crate::prompt::step::STEPS_DIR)
        .join(agent_id)
        .join(crate::prompt::step::DRIVER_LOG_FILE)
}

#[test]
fn detach_into_own_session_never_fails() {
    // The pre-exec hook, called in-process: `setsid` from a process
    // that already leads a group reports EPERM, which the hook ignores
    // by contract — the driver spawn proceeds grouped either way. The
    // in-process call is what puts the hook's lines in the coverage
    // numerator (counters incremented in the forked child are lost at
    // exec).
    super::super::detach_into_own_session().unwrap();
    super::super::detach_into_own_session().unwrap();
}

#[test]
fn advance_launcher_surfaces_a_spawn_failure() {
    let ws = TempDir::new().unwrap();
    let launcher = AdvanceLauncher::with_exe("/no/such/litany-binary".into());
    let err = launcher.launch(ws.path(), "a1").unwrap_err();
    assert_eq!(err.kind(), io::ErrorKind::NotFound);
}
