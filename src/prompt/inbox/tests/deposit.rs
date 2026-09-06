//! Deposit tests (ARCH §2.11 *Deposit*): create-only atomicity,
//! frontmatter, and sender sequence derivation.

use super::super::deposit::{DepositError, atomic_create, deposit, next_sequence};
use super::super::inbox_dir;
use crate::prompt::Clock;
use crate::template::RealGit;
use std::path::Path;
use tempfile::TempDir;

/// Fixed [`Clock`] — deposits read only `now_iso8601`.
struct FixedClock;
impl Clock for FixedClock {
    fn now_iso8601(&self) -> String {
        "2026-07-11T00:00:00Z".into()
    }
    fn now_compact(&self) -> String {
        unreachable!("deposit never reads the compact clock")
    }
}

fn read(path: &Path) -> String {
    std::fs::read_to_string(path).unwrap()
}

#[test]
fn deposit_writes_named_file_with_frontmatter_and_body() {
    let ws = TempDir::new().unwrap();
    let path = deposit(
        ws.path(),
        "p1-child",
        "user",
        "steer left\n",
        &FixedClock,
        &RealGit::new(),
    )
    .unwrap();

    assert_eq!(path, inbox_dir(ws.path(), "p1-child").join("user-001.md"));
    assert_eq!(
        read(&path),
        "---\nfrom: user\ndeposited_at: 2026-07-11T00:00:00Z\n---\nsteer left\n"
    );
}

#[test]
fn deposit_creates_inbox_directory_on_demand() {
    let ws = TempDir::new().unwrap();
    assert!(!inbox_dir(ws.path(), "a1").exists());
    deposit(ws.path(), "a1", "user", "hi", &FixedClock, &RealGit::new()).unwrap();
    assert!(inbox_dir(ws.path(), "a1").is_dir());
}

#[test]
fn same_sender_sequence_increments() {
    let ws = TempDir::new().unwrap();
    let p1 = deposit(ws.path(), "a1", "user", "one", &FixedClock, &RealGit::new()).unwrap();
    let p2 = deposit(ws.path(), "a1", "user", "two", &FixedClock, &RealGit::new()).unwrap();
    let p3 = deposit(
        ws.path(),
        "a1",
        "user",
        "three",
        &FixedClock,
        &RealGit::new(),
    )
    .unwrap();
    assert!(p1.ends_with("user-001.md"));
    assert!(p2.ends_with("user-002.md"));
    assert!(p3.ends_with("user-003.md"));
    // Create-only: every deposit is its own file, none clobbered.
    for p in [&p1, &p2, &p3] {
        assert!(p.exists());
    }
}

#[test]
fn distinct_senders_keep_independent_sequences() {
    let ws = TempDir::new().unwrap();
    // Two senders into one inbox: each numbers from 001, no collision
    // (sender-namespacing, §2.11).
    let a = deposit(ws.path(), "a1", "user", "u", &FixedClock, &RealGit::new()).unwrap();
    let b = deposit(
        ws.path(),
        "a1",
        "p2-agent",
        "g",
        &FixedClock,
        &RealGit::new(),
    )
    .unwrap();
    let a2 = deposit(ws.path(), "a1", "user", "u2", &FixedClock, &RealGit::new()).unwrap();
    assert!(a.ends_with("user-001.md"));
    assert!(b.ends_with("p2-agent-001.md"));
    assert!(a2.ends_with("user-002.md"));
}

#[test]
fn no_tmp_file_survives_a_deposit() {
    let ws = TempDir::new().unwrap();
    deposit(ws.path(), "a1", "user", "hi", &FixedClock, &RealGit::new()).unwrap();
    let dir = inbox_dir(ws.path(), "a1");
    let names: Vec<String> = std::fs::read_dir(&dir)
        .unwrap()
        .flatten()
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .collect();
    assert_eq!(
        names,
        vec!["user-001.md".to_string()],
        "no .tmp left behind"
    );
}

#[test]
fn next_sequence_ignores_foreign_and_malformed_names() {
    let ws = TempDir::new().unwrap();
    let dir = inbox_dir(ws.path(), "a1");
    std::fs::create_dir_all(&dir).unwrap();
    // A longer-id sender's file, a non-numeric stray, another sender —
    // none count toward `user`'s sequence.
    for name in [
        "user-abc.md",
        "user-agent-001.md",
        "p2-005.md",
        "user-007.md",
    ] {
        std::fs::write(dir.join(name), b"x").unwrap();
    }
    // Only `user-007.md` is a legible `user` deposit → next is 008.
    assert_eq!(next_sequence(&dir, "user").unwrap(), 8);
}

#[test]
fn next_sequence_surfaces_read_dir_error() {
    // A directory that does not exist → read_dir errors (the outer `?`).
    let ws = TempDir::new().unwrap();
    let missing = ws.path().join("nope");
    assert!(next_sequence(&missing, "user").is_err());
}

#[test]
fn deposit_surfaces_io_error_when_inbox_home_blocked() {
    let ws = TempDir::new().unwrap();
    std::fs::write(ws.path().join("inbox"), b"not a dir").unwrap();
    let err = deposit(ws.path(), "a1", "user", "hi", &FixedClock, &RealGit::new()).unwrap_err();
    assert!(matches!(err, DepositError::Io { .. }), "{err}");
}

#[test]
fn atomic_create_surfaces_write_error() {
    // Parent directory absent → the temp write fails.
    let err = atomic_create(Path::new("/no/such/dir"), "user-001.md", b"x").unwrap_err();
    let DepositError::Io { path, .. } = err else {
        panic!("expected Io, got {err}");
    };
    assert!(path.ends_with(".user-001.md.tmp"), "{}", path.display());
}

#[test]
fn atomic_create_surfaces_rename_error() {
    let ws = TempDir::new().unwrap();
    let dir = ws.path();
    // A non-empty directory at the final name makes rename fail.
    let blocking = dir.join("user-001.md");
    std::fs::create_dir(&blocking).unwrap();
    std::fs::write(blocking.join("occupied"), b"x").unwrap();
    let err = atomic_create(dir, "user-001.md", b"x").unwrap_err();
    let DepositError::Io { path, .. } = err else {
        panic!("expected Io, got {err}");
    };
    assert!(path.ends_with("user-001.md"), "{}", path.display());
}
