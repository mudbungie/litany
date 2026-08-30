//! Unit tests for the `litany advance` production wiring
//! ([`super`]): the §2.3 existence guard, the lease arms (acquire,
//! held, adopted, bad fd), the layout guard, and the §6 handoff mapping.
//! Lifted out of the module to keep it under the 300-line cap.

use super::*;
use crate::prompt::inbox::{ExecutorLock, inbox_dir};
use tempfile::TempDir;

/// Take a lease for tests: acquire on a scratch inbox, or die trying.
fn test_lease(dir: &Path) -> ExecutorLock {
    inbox::try_acquire(dir).unwrap().expect("free lock")
}

/// The injected driver target for tests — a bare name; these hops all
/// error before any spawn/exec would consult it.
fn td() -> &'static Path {
    Path::new("litany")
}

#[test]
fn a_non_workspace_is_refused_by_the_layout_guard() {
    // Pre-v1 clean break (§2.2, §10): the guard fires before any
    // lease or inbox work.
    let ws = TempDir::new().unwrap();
    let err = cli_run(
        ws.path(),
        "20260101-a1",
        td(),
        None,
        &AtomicBool::new(false),
        None,
    )
    .unwrap_err();
    assert!(matches!(err, Error::Layout(_)), "{err}");
}

#[test]
fn a_name_with_no_agent_ref_is_refused_before_the_lease() {
    // The §2.3 existence guard: no `agents/*` ref, so the hop is
    // refused in the `litany message` voice — and, crucially, the
    // inbox directory the lease would have created never appears.
    let (_h, ws) = crate::workspace::fixture::workspace();
    let err = cli_run(&ws, "ghost", td(), None, &AtomicBool::new(false), None).unwrap_err();
    assert!(matches!(err, Error::UnknownAgent(_)), "{err}");
    assert!(err.to_string().starts_with("no agent \"ghost\""), "{err}");
    assert!(
        !inbox_dir(&ws, "ghost").exists(),
        "a refused hop mints no inbox directory"
    );
}

#[test]
fn a_quiescent_agent_is_nothing_to_do_via_production_wiring() {
    let (_h, ws) = crate::workspace::fixture::workspace();
    crate::workspace::fixture::spawn_root(&ws, "20260101-a1");
    let out = cli_run(
        &ws,
        "20260101-a1",
        td(),
        None,
        &AtomicBool::new(false),
        None,
    )
    .unwrap();
    assert!(matches!(out, AdvanceHandoff::Done));
}

#[test]
fn held_lock_is_already_driven() {
    let (_h, ws) = crate::workspace::fixture::workspace();
    crate::workspace::fixture::spawn_root(&ws, "20260101-a1");
    let _held = test_lease(&inbox_dir(&ws, "20260101-a1"));
    let out = cli_run(
        &ws,
        "20260101-a1",
        td(),
        None,
        &AtomicBool::new(false),
        None,
    )
    .unwrap();
    assert!(matches!(out, AdvanceHandoff::Done));
}

#[test]
fn broken_inbox_surfaces_as_executor_lock_error() {
    let (_h, ws) = crate::workspace::fixture::workspace();
    crate::workspace::fixture::spawn_root(&ws, "20260101-a1");
    std::fs::create_dir_all(ws.join("inbox")).unwrap();
    std::fs::write(inbox_dir(&ws, "20260101-a1"), b"not a dir").unwrap();
    let err = cli_run(
        &ws,
        "20260101-a1",
        td(),
        None,
        &AtomicBool::new(false),
        None,
    )
    .unwrap_err();
    assert!(matches!(err, Error::ExecutorLock { .. }), "{err}");
}

#[test]
fn bad_lease_env_is_declined_loudly_as_lease_adopt() {
    let (_h, ws) = crate::workspace::fixture::workspace();
    crate::workspace::fixture::spawn_root(&ws, "20260101-a1");
    std::fs::create_dir_all(inbox_dir(&ws, "20260101-a1")).unwrap();
    let err = cli_run_with(
        &ws,
        "20260101-a1",
        Some(OsStr::new("not-an-fd")),
        td(),
        None,
        &AtomicBool::new(false),
        None,
    )
    .unwrap_err();
    assert!(matches!(err, Error::LeaseAdopt { .. }), "{err}");
}

#[test]
fn adopted_lease_env_drives_the_hop() {
    // Simulate the predecessor: acquire, publish the fd number, and
    // leak the guard (exactly what `successor_command` does before
    // exec). The adopting hop finds nothing due on the empty branch.
    let (_h, ws) = crate::workspace::fixture::workspace();
    crate::workspace::fixture::spawn_root(&ws, "20260101-a1");
    let dir = inbox_dir(&ws, "20260101-a1");
    let lease = test_lease(&dir);
    let fd = lease.as_raw_fd().to_string();
    std::mem::forget(lease);
    let out = cli_run_with(
        &ws,
        "20260101-a1",
        Some(OsStr::new(&fd)),
        td(),
        None,
        &AtomicBool::new(false),
        None,
    )
    .unwrap();
    assert!(matches!(out, AdvanceHandoff::Done));
}

#[test]
fn a_warranted_hop_delivers_then_consults_the_resolver() {
    // A real branch with pending mail: the hop delivers (real git),
    // finds the tail user-side, and consults the production resolver —
    // loud against the test-machine harness root (a missing global
    // models.yaml, a version-skewed `bz`, or a credential-less spawn,
    // whichever the machine hits first). That the delivery landed
    // before the hop failed is the §6 lazy-resolution ordering,
    // observed on disk.
    let (_h, ws) = crate::workspace::fixture::workspace();
    let agent = "20260101-a1";
    let wt = crate::workspace::fixture::spawn_root(&ws, agent);
    inbox::deposit(&ws, agent, "user", "hi", &SystemClock).unwrap();
    let err = cli_run(&ws, agent, td(), None, &AtomicBool::new(false), None).unwrap_err();
    assert!(!err.to_string().is_empty());
    // The delivery commit landed ahead of the failed resolution.
    assert!(wt.join("messages/001-user.md").exists());
}

#[test]
fn tools_pending_hands_off_as_a_prepared_exec() {
    let ws = TempDir::new().unwrap();
    let lease = test_lease(&inbox_dir(ws.path(), "20260101-a1"));
    let out = handoff(
        Path::new("/usr/bin/litany"),
        ws.path(),
        "20260101-a1",
        AdvanceOutcome::ToolsPending(lease),
    )
    .unwrap();
    let AdvanceHandoff::Exec(cmd) = out else {
        panic!("expected Exec");
    };
    let args: Vec<_> = cmd.get_args().map(|a| a.to_os_string()).collect();
    assert_eq!(args[0], "advance");
    assert!(
        cmd.get_envs()
            .any(|(k, v)| k == baton::LOCK_FD_ENV && v.is_some())
    );
}

#[test]
fn non_tools_outcomes_hand_off_as_done() {
    let ws = TempDir::new().unwrap();
    let out = handoff(
        Path::new("litany"),
        ws.path(),
        "20260101-a1",
        AdvanceOutcome::NothingToDo,
    )
    .unwrap();
    assert!(matches!(out, AdvanceHandoff::Done));
}
