//! The real [`AdvanceLauncher`] — the detached `litany advance` spawn
//! itself (ARCH §2.11), split from [`super::probe`] because it is a
//! different axis: probe answers *whether* to launch, this answers what
//! a launch DOES to the process table and to `steps/<agent-id>/
//! driver.log`. Every beat here forks a real child.

use super::super::{AdvanceLauncher, Launcher};
use std::io;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use tempfile::TempDir;

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
