use super::*;
use std::os::unix::fs::symlink;
use std::path::PathBuf;
use tempfile::TempDir;

fn write(p: &Path, body: &str) {
    if let Some(parent) = p.parent() {
        std::fs::create_dir_all(parent).unwrap();
    }
    std::fs::write(p, body).unwrap();
}

/// Lay out a synthetic `/proc/<pid>/{fd,stat}` plus a real on-disk inbox
/// directory so `canonicalize` resolves cleanly and `<pid>/fd/<fd>`
/// symlinks to it — the executor-lock fd (§2.11). The holder is a
/// process-group **leader** (`pgid == pid`), which is what a settled
/// executor always is (§2.9) and the only shape discovery trusts;
/// [`fixture_with_pgid`] builds the unsettled counter-case.
fn fixture(pid: i32, fd: u32) -> (TempDir, PathBuf) {
    fixture_with_pgid(pid, fd, pid)
}

/// [`fixture`] with the pgid stated separately, so a test can build a
/// holder that still reports its spawner's group. Returns (tmp,
/// inbox_dir).
fn fixture_with_pgid(pid: i32, fd: u32, pgid: i32) -> (TempDir, PathBuf) {
    let dir = TempDir::new().unwrap();
    let inbox = dir.path().join("inbox").join("agent-1");
    std::fs::create_dir_all(&inbox).unwrap();
    let proc_root = dir.path().join("proc");
    let fd_dir = proc_root.join(pid.to_string()).join("fd");
    std::fs::create_dir_all(&fd_dir).unwrap();
    symlink(&inbox, fd_dir.join(fd.to_string())).unwrap();
    write_stat(&proc_root, pid, pgid);
    (dir, inbox)
}

/// A `/proc/<pid>/stat` line reporting `pgid` — the one field discovery
/// reads, in its `proc(5)` position after the parenthesized comm.
fn stat_line(pid: i32, pgid: i32) -> String {
    format!("{pid} (litany) S 1 {pgid} 0 0 -1 0 0 0 0 0 0 0 0 0 20 0 1 0\n")
}

fn write_stat(proc_root: &Path, pid: i32, pgid: i32) {
    write(
        &proc_root.join(pid.to_string()).join("stat"),
        &stat_line(pid, pgid),
    );
}

#[test]
fn finder_returns_none_when_inbox_dir_missing() {
    let dir = TempDir::new().unwrap();
    let f = ProcFsFinder::with_root(dir.path().join("proc-empty"));
    let result = f.find_holder_pgid(&dir.path().join("missing")).unwrap();
    assert!(result.is_none());
}

#[test]
fn finder_returns_none_when_no_pid_holds_the_inbox() {
    // The inbox dir exists (canonicalize succeeds) but the sole pid in
    // the fixture holds an fd on an unrelated path — the scan exhausts
    // without a match and returns None (the "already-stopped" path).
    let dir = TempDir::new().unwrap();
    let inbox = dir.path().join("inbox").join("agent-1");
    std::fs::create_dir_all(&inbox).unwrap();
    let elsewhere = dir.path().join("elsewhere");
    std::fs::create_dir_all(&elsewhere).unwrap();
    let fd_dir = dir.path().join("proc").join("5555").join("fd");
    std::fs::create_dir_all(&fd_dir).unwrap();
    symlink(&elsewhere, fd_dir.join("3")).unwrap();
    let f = ProcFsFinder::with_root(dir.path().join("proc"));
    assert!(f.find_holder_pgid(&inbox).unwrap().is_none());
}

#[test]
fn finder_default_points_at_proc() {
    // Default impl is what production uses. Smoke-check the
    // root, not actual scanning — `/proc` is real on the host
    // but we don't want a unit test to depend on its contents.
    let f = ProcFsFinder::default();
    assert_eq!(f.proc_root, std::path::PathBuf::from("/proc"));
}

#[test]
fn finder_propagates_canonicalize_error_other_than_not_found() {
    // Canonicalize on a path whose intermediate component is a
    // regular file (not a directory) returns NotADirectory, not
    // NotFound — exercises the catch-all error branch.
    let dir = TempDir::new().unwrap();
    let regular = dir.path().join("regular");
    std::fs::write(&regular, "x").unwrap();
    let through_file = regular.join("inside").join("inbox");
    let f = ProcFsFinder::with_root(dir.path().join("proc-empty"));
    let err = f.find_holder_pgid(&through_file).unwrap_err();
    assert_ne!(err.kind(), io::ErrorKind::NotFound);
}

#[test]
fn finder_returns_pgid_for_inbox_fd_holder() {
    let (dir, inbox) = fixture(1234, 7);
    let f = ProcFsFinder::with_root(dir.path().join("proc"));
    let pgid = f.find_holder_pgid(&inbox).unwrap();
    assert_eq!(pgid, Some(1234));
}

#[test]
fn finder_skips_unparseable_pid_dirs() {
    let (dir, inbox) = fixture(1234, 7);
    // Add non-numeric dirs like `/proc/self` or `/proc/sys`.
    std::fs::create_dir_all(dir.path().join("proc").join("sys")).unwrap();
    std::fs::create_dir_all(dir.path().join("proc").join("self")).unwrap();
    let f = ProcFsFinder::with_root(dir.path().join("proc"));
    let pgid = f.find_holder_pgid(&inbox).unwrap();
    assert_eq!(pgid, Some(1234));
}

#[test]
fn finder_skips_fd_with_unrelated_target() {
    let (dir, inbox) = fixture(1234, 7);
    // A second pid whose fd points elsewhere — must not match.
    let other_target = dir.path().join("other");
    std::fs::create_dir_all(&other_target).unwrap();
    let other_fd_dir = dir.path().join("proc").join("5555").join("fd");
    std::fs::create_dir_all(&other_fd_dir).unwrap();
    symlink(&other_target, other_fd_dir.join("3")).unwrap();
    let f = ProcFsFinder::with_root(dir.path().join("proc"));
    let pgid = f.find_holder_pgid(&inbox).unwrap();
    assert_eq!(pgid, Some(1234));
}

#[test]
fn finder_skips_pid_dir_with_no_fd_subdir() {
    let (dir, inbox) = fixture(1234, 7);
    // Pid with stat but no fd dir → unread; not a match.
    std::fs::create_dir_all(dir.path().join("proc").join("7777")).unwrap();
    let f = ProcFsFinder::with_root(dir.path().join("proc"));
    let pgid = f.find_holder_pgid(&inbox).unwrap();
    assert_eq!(pgid, Some(1234));
}

#[test]
fn finder_skips_non_symlink_fd_entry() {
    let (dir, inbox) = fixture(1234, 7);
    // A regular file under another pid's fd dir: `read_link` errors
    // (not a symlink) → the entry is skipped rather than fatal.
    let other_fd_dir = dir.path().join("proc").join("6666").join("fd");
    std::fs::create_dir_all(&other_fd_dir).unwrap();
    std::fs::write(other_fd_dir.join("0"), "not a symlink").unwrap();
    let f = ProcFsFinder::with_root(dir.path().join("proc"));
    let pgid = f.find_holder_pgid(&inbox).unwrap();
    assert_eq!(pgid, Some(1234));
}

#[test]
fn read_pgid_errors_on_malformed_stat_no_close_paren() {
    let dir = TempDir::new().unwrap();
    write(
        &dir.path().join("proc").join("1234").join("stat"),
        "1234 litany S 1 9999 0\n",
    );
    let err = read_pgid(&dir.path().join("proc"), 1234).unwrap_err();
    assert_eq!(err.kind(), io::ErrorKind::InvalidData);
}

#[test]
fn read_pgid_errors_on_truncated_fields() {
    let dir = TempDir::new().unwrap();
    write(
        &dir.path().join("proc").join("1234").join("stat"),
        "1234 (litany) S 1\n",
    );
    let err = read_pgid(&dir.path().join("proc"), 1234).unwrap_err();
    assert_eq!(err.kind(), io::ErrorKind::InvalidData);
}

#[test]
fn read_pgid_errors_on_unparseable_pgid() {
    let dir = TempDir::new().unwrap();
    write(
        &dir.path().join("proc").join("1234").join("stat"),
        "1234 (litany) S 1 not-a-number 0\n",
    );
    let err = read_pgid(&dir.path().join("proc"), 1234).unwrap_err();
    assert_eq!(err.kind(), io::ErrorKind::InvalidData);
}

#[test]
fn finder_treats_fd_with_unparseable_name_as_skip() {
    let (dir, inbox) = fixture(1234, 7);
    // A non-numeric fd entry still symlinks to the inbox; the scan
    // matches on the target, not the fd name, so it is found via the
    // numeric fd 7 regardless.
    symlink(
        &inbox,
        dir.path()
            .join("proc")
            .join("1234")
            .join("fd")
            .join("notanum"),
    )
    .unwrap();
    let f = ProcFsFinder::with_root(dir.path().join("proc"));
    let pgid = f.find_holder_pgid(&inbox).unwrap();
    assert_eq!(pgid, Some(1234));
}

#[test]
fn finder_refuses_a_holder_that_still_reports_its_spawners_group() {
    // The bl-5f0c race, frozen: the lock holder is pid 1234 but its
    // stat still names group 9999 — the group it inherited from
    // whoever spawned it, because its own setpgid/setsid has not
    // landed. Signalling 9999 fells the spawner's whole tree (the
    // coverage runner, under `make check`). Discovery must refuse
    // rather than hand that reading to the cascade.
    //
    // Two retries, zero backoff: the retries are what exercise the
    // re-read arm, and the fixture never changes, so the budget is
    // guaranteed to exhaust without waiting on any clock.
    let (dir, inbox) = fixture_with_pgid(1234, 7, 9999);
    let f = ProcFsFinder::with_root(dir.path().join("proc"))
        .with_leader_retry(2, std::time::Duration::ZERO);
    let err = f.find_holder_pgid(&inbox).unwrap_err();
    let msg = err.to_string();
    assert!(msg.contains("9999"), "must name the rejected group: {msg}");
    assert!(
        msg.contains("Refusing to signal it"),
        "must say no signal was sent: {msg}"
    );
}

#[test]
fn finder_retries_until_the_holders_setpgid_lands() {
    // The same unsettled holder, but this one settles: a writer flips
    // `/proc/1234/stat` to the leader form while discovery is
    // re-reading, and the retry rides that out instead of refusing.
    //
    // The rename is atomic so no read can observe a torn file, and
    // the retry budget is enormous against a 5 ms flip for the reason
    // bl-7a3f pinned: a budget racing a fixture on another clock
    // reports machine load, not code. If the flip lands before the
    // first read the assertion still holds — this test asserts the
    // outcome; the re-read arm itself is covered by the refusal test
    // above.
    let (dir, inbox) = fixture_with_pgid(1234, 7, 9999);
    let proc_root = dir.path().join("proc");
    let settle = proc_root.join("1234");
    let writer = std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_millis(5));
        let staged = settle.join("stat.settled");
        write(&staged, &stat_line(1234, 1234));
        std::fs::rename(&staged, settle.join("stat")).unwrap();
    });
    let f = ProcFsFinder::with_root(proc_root)
        .with_leader_retry(100_000, std::time::Duration::from_millis(1));
    let pgid = f.find_holder_pgid(&inbox).unwrap();
    writer.join().unwrap();
    assert_eq!(pgid, Some(1234), "a settled leader's pgid is its own pid");
}
