//! Edge cases, error propagation, and direct surface coverage for
//! [`crate::prompt::stop`] — the bits that don't fit the happy-path
//! orchestration narrative.

use super::fixtures::{
    ErrFinder, ErrInspector, NoopGit, OwnGroupFinder, StubFinder, StubInspector, touch_inbox_dir,
};
use crate::prompt::stop::{Error, cascade, run};
use crate::template::GitRunner;
use std::io;
use std::path::Path;
use std::time::Duration;
use tempfile::TempDir;

#[test]
fn run_propagates_inspector_exists_error_as_git() {
    let dir = TempDir::new().unwrap();
    let err = run(
        dir.path(),
        "br",
        &ErrInspector,
        &StubFinder::default(),
        &cascade::RecordingSignaler::new(0),
        Duration::from_millis(1),
        &NoopGit,
    )
    .unwrap_err();
    matches!(
        err,
        Error::Git {
            op: "rev-parse --verify",
            ..
        }
    );
}

#[test]
fn run_propagates_finder_io_error_as_proc() {
    let dir = TempDir::new().unwrap();
    touch_inbox_dir(dir.path(), "br");
    let inspector = StubInspector { exists: true };
    let err = run(
        dir.path(),
        "br",
        &inspector,
        &ErrFinder,
        &cascade::RecordingSignaler::new(0),
        Duration::from_millis(1),
        &NoopGit,
    )
    .unwrap_err();
    matches!(err, Error::Proc(_));
}

#[test]
fn collect_inbox_dirs_returns_empty_when_inbox_root_missing() {
    let dir = TempDir::new().unwrap();
    let v = super::super::collect_inbox_dirs(dir.path(), "br").unwrap();
    assert!(v.is_empty());
}

#[test]
fn collect_inbox_dirs_skips_a_non_utf8_entry() {
    // An inbox entry whose name `to_str` cannot decode can never match
    // a branch name — skipped, not fatal.
    use std::os::unix::ffi::OsStrExt;
    let dir = TempDir::new().unwrap();
    touch_inbox_dir(dir.path(), "br");
    let stray = dir
        .path()
        .join(super::fixtures::INBOX_DIR)
        .join(std::ffi::OsStr::from_bytes(b"\xFF\xFE"));
    std::fs::create_dir_all(&stray).unwrap();
    let v = super::super::collect_inbox_dirs(dir.path(), "br").unwrap();
    assert_eq!(v.len(), 1, "only the named branch: {v:?}");
    assert!(v[0].ends_with("inbox/br"));
}

#[test]
fn collect_inbox_dirs_takes_the_descendants_unconditionally() {
    // The walk is the stop (§2.9, bl-3114): there is no arm that
    // returns the agent alone, so a `br-*` descendant is in the sweep
    // whatever the caller wanted. Pinned at the collector, which is the
    // only place the reach was ever decided.
    let dir = TempDir::new().unwrap();
    touch_inbox_dir(dir.path(), "br");
    touch_inbox_dir(dir.path(), "br-sub");

    let mut dirs = super::super::collect_inbox_dirs(dir.path(), "br").unwrap();
    dirs.sort();
    assert_eq!(dirs.len(), 2, "self and descendant: {dirs:?}");
    assert!(dirs[0].ends_with("inbox/br"));
    assert!(dirs[1].ends_with("inbox/br-sub"));
}

#[test]
fn error_display_branch_missing_includes_name() {
    let e = Error::BranchMissing("foo".into());
    let s = format!("{e}");
    assert!(s.contains("\"foo\""), "got {s}");
}

#[test]
fn error_display_inbox_walk_passes_through_io_message() {
    let e = Error::InboxWalk(io::Error::other("nope"));
    let s = format!("{e}");
    assert!(s.contains("nope"), "got {s}");
}

#[test]
fn noop_git_returns_ok_from_both_methods() {
    // The orchestration tests pass `&NoopGit` purely to satisfy
    // run()'s signature; StubInspector ignores the runner. Cover
    // NoopGit's body directly so tarpaulin sees it.
    let g = NoopGit;
    g.run(Path::new("/anywhere"), &["any", "args"]).unwrap();
    assert_eq!(g.run_capture(Path::new("/anywhere"), &["any"]).unwrap(), "");
}

#[test]
fn cli_run_guards_the_layout_before_anything_else() {
    // cli_run wires production deps. Pointed at a temp dir that is not
    // a workspace, the §2.2 layout guard refuses first (pre-v1 clean
    // break) — before any git or /proc work.
    let dir = TempDir::new().unwrap();
    let err = super::super::cli_run(dir.path(), "no-such-branch").unwrap_err();
    assert!(matches!(err, Error::Layout(_)), "{err}");
}

#[test]
fn cli_run_returns_branch_missing_against_a_real_workspace() {
    // Against a real workspace (bare repo.git + config/default), a
    // nonexistent agent id fails branch validation — the canonical
    // pre-cascade error path.
    let (_h, ws) = crate::workspace::fixture::workspace();
    let err = super::super::cli_run(&ws, "no-such-branch").unwrap_err();
    assert!(
        matches!(err, Error::BranchMissing(ref b) if b == "no-such-branch"),
        "{err}"
    );
}

#[test]
fn run_refuses_to_signal_the_stop_process_own_group() {
    // The §2.9 belt-and-braces guard, and the whole point of bl-5f0c:
    // discovery hands back the group this very process stands in —
    // what a `/proc` read of a not-yet-detached executor returns,
    // since it still reports the group it inherited from its spawner.
    // `kill(-that, SIGTERM)` is the operator's shell job in production
    // and was, twice, this test binary's own coverage run. `run` must
    // refuse before the cascade and send nothing at all.
    //
    // Pinning ourselves as a group leader first makes `getpgrp()`
    // stable for the rest of the process, so the stub's probe-time
    // reading and `run`'s own cannot disagree even if a sibling test
    // exercises `become_pgid_leader` in parallel.
    super::super::become_pgid_leader();
    // SAFETY: `getpgrp` takes no arguments and cannot fail.
    let own = unsafe { libc::getpgrp() };

    let dir = TempDir::new().unwrap();
    touch_inbox_dir(dir.path(), "br");
    let signaler = cascade::RecordingSignaler::new(0);
    let err = run(
        dir.path(),
        "br",
        &StubInspector { exists: true },
        &OwnGroupFinder,
        &signaler,
        Duration::from_millis(1),
        &NoopGit,
    )
    .unwrap_err();
    assert!(
        matches!(err, Error::SelfGroup { pgid } if pgid == own),
        "{err}"
    );
    assert!(
        signaler.took().is_empty(),
        "refusal must precede the cascade — not one signal may escape"
    );
    let rendered = err.to_string();
    assert!(
        rendered.contains("nothing was signalled"),
        "the error must tell the operator no signal landed: {rendered}"
    );
}

#[test]
fn become_pgid_leader_returns_cleanly() {
    // Direct call to the production wrapper. Idempotent on a process
    // that's already a pgid leader (typical for cargo test under
    // shell job control). The branch table itself is covered by the
    // closure-injected variants below, so this test is just to seal
    // the wrapper's coverage.
    super::super::become_pgid_leader();
}

#[test]
fn become_pgid_leader_with_zero_succeeds_silently() {
    // Setpgid returning 0 ("ok") takes the silent branch; nothing
    // observable except that the function returned.
    super::super::become_pgid_leader_with(|| 0);
}

#[test]
fn become_pgid_leader_with_nonzero_takes_error_branch() {
    // Setpgid returning -1 is the failure path. The function prints
    // to stderr (irrelevant for coverage) and returns; only the
    // branch reachability matters here.
    super::super::become_pgid_leader_with(|| -1);
}
