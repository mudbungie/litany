//! Lease-handoff tests (§6 "The exec baton carries the lease"): fd
//! adoption with fstat validation, close-on-exec restore/clear, the
//! successor command's published environment, and every decline arm.

use super::{
    AdoptError, LOCK_FD_ENV, adopt, interpret_cloexec, interpret_flock, set_fd_flags, take_lease,
};
use crate::prompt::inbox::{inbox_dir, try_acquire};
use std::ffi::OsStr;
use std::fs::File;
use std::io;
use std::os::fd::{FromRawFd, IntoRawFd, RawFd};
use std::path::Path;
use tempfile::TempDir;

/// Open `dir` and leak the fd — the shape a predecessor hop leaves
/// behind: an open, inheritable fd whose number rides the environment.
fn leaked_dir_fd(dir: &Path) -> RawFd {
    File::open(dir).unwrap().into_raw_fd()
}

/// Read the fd's FD_CLOEXEC flag.
fn cloexec_of(fd: RawFd) -> bool {
    // SAFETY: F_GETFD on a live fd; no memory effects.
    let flags = unsafe { libc::fcntl(fd, libc::F_GETFD) };
    assert!(flags != -1, "fcntl F_GETFD failed");
    flags & libc::FD_CLOEXEC != 0
}

#[test]
fn adopt_validates_and_restores_cloexec() {
    let ws = TempDir::new().unwrap();
    let dir = inbox_dir(ws.path(), "a1");
    std::fs::create_dir_all(&dir).unwrap();
    let fd = leaked_dir_fd(&dir);
    // The predecessor cleared close-on-exec before exec (§6).
    set_fd_flags(fd, 0).unwrap();
    let lock = adopt(OsStr::new(&fd.to_string()), &dir)
        .unwrap()
        .expect("adopted");
    // Restored: the lease never rides into tool/adapter subprocesses.
    assert!(cloexec_of(lock.as_raw_fd()));
}

#[test]
fn adopt_declines_a_non_numeric_env_value() {
    let ws = TempDir::new().unwrap();
    let err = adopt(OsStr::new("not-a-number"), ws.path()).unwrap_err();
    assert!(matches!(err, AdoptError::Parse(_)), "{err}");
}

#[test]
fn adopt_declines_a_closed_fd() {
    let ws = TempDir::new().unwrap();
    let err = adopt(OsStr::new("999999"), ws.path()).unwrap_err();
    assert!(matches!(err, AdoptError::Fstat { .. }), "{err}");
}

#[test]
fn adopt_declines_when_the_inbox_is_unreadable() {
    let ws = TempDir::new().unwrap();
    let fd = leaked_dir_fd(ws.path());
    let missing = ws.path().join("no-such-inbox");
    let err = adopt(OsStr::new(&fd.to_string()), &missing).unwrap_err();
    assert!(matches!(err, AdoptError::InboxStat { .. }), "{err}");
    // The fd was not stolen by the decline: still open and usable.
    // SAFETY: F_GETFD on the still-open fd.
    assert!(unsafe { libc::fcntl(fd, libc::F_GETFD) } != -1);
    // SAFETY: reclaim the deliberately leaked fd.
    drop(unsafe { File::from_raw_fd(fd) });
}

#[test]
fn adopt_declines_a_mismatched_fd_without_stealing_it() {
    let ws = TempDir::new().unwrap();
    let other = ws.path().join("other");
    let dir = inbox_dir(ws.path(), "a1");
    std::fs::create_dir_all(&other).unwrap();
    std::fs::create_dir_all(&dir).unwrap();
    let fd = leaked_dir_fd(&other);
    let err = adopt(OsStr::new(&fd.to_string()), &dir).unwrap_err();
    assert!(matches!(err, AdoptError::Mismatch { .. }), "{err}");
    // Declined without closing: the fd still answers fcntl.
    // SAFETY: F_GETFD on the still-open fd.
    assert!(unsafe { libc::fcntl(fd, libc::F_GETFD) } != -1);
    // SAFETY: reclaim the deliberately leaked fd.
    drop(unsafe { File::from_raw_fd(fd) });
}

#[test]
fn adopt_resolves_a_contended_lease_to_already_driven() {
    // A second open file description on the same directory does not
    // hold the flock; the adopt re-assert observes the real holder and
    // yields None — the Writer/driver-totality no-op, not an error.
    let ws = TempDir::new().unwrap();
    let dir = inbox_dir(ws.path(), "a1");
    let _held = try_acquire(&dir).unwrap().expect("free");
    let fd = leaked_dir_fd(&dir);
    let out = adopt(OsStr::new(&fd.to_string()), &dir).unwrap();
    assert!(out.is_none());
}

#[test]
fn take_lease_acquires_when_no_env_is_published() {
    let ws = TempDir::new().unwrap();
    let dir = inbox_dir(ws.path(), "a1");
    let lock = take_lease(None, &dir).unwrap().expect("acquired");
    drop(lock);
}

#[test]
fn take_lease_surfaces_an_acquire_failure() {
    let ws = TempDir::new().unwrap();
    std::fs::create_dir_all(ws.path().join("inbox")).unwrap();
    let broken = inbox_dir(ws.path(), "a1");
    std::fs::write(&broken, b"not a dir").unwrap();
    let err = take_lease(None, &broken).unwrap_err();
    assert!(matches!(err, super::LeaseError::Acquire(_)), "{err}");
}

#[test]
fn take_lease_adopts_when_the_env_is_published() {
    let ws = TempDir::new().unwrap();
    let dir = inbox_dir(ws.path(), "a1");
    std::fs::create_dir_all(&dir).unwrap();
    let fd = leaked_dir_fd(&dir);
    let lock = take_lease(Some(OsStr::new(&fd.to_string())), &dir)
        .unwrap()
        .expect("adopted");
    assert_eq!(lock.as_raw_fd(), fd);
}

#[test]
fn successor_command_publishes_an_inheritable_fd() {
    let ws = TempDir::new().unwrap();
    let dir = inbox_dir(ws.path(), "a1");
    let lease = try_acquire(&dir).unwrap().expect("free");
    let fd = lease.as_raw_fd();
    let cmd =
        super::successor_command(Path::new("/usr/bin/litany"), ws.path(), "a1", lease).unwrap();
    // Close-on-exec cleared: the one deliberate inheritance (§6).
    assert!(!cloexec_of(fd));
    let args: Vec<_> = cmd.get_args().map(|a| a.to_os_string()).collect();
    assert_eq!(args[0], "advance");
    assert_eq!(args[2], "a1");
    let published = cmd
        .get_envs()
        .find(|(k, _)| *k == OsStr::new(LOCK_FD_ENV))
        .and_then(|(_, v)| v.map(|v| v.to_os_string()))
        .expect("LITANY_LOCK_FD published");
    assert_eq!(published, OsStr::new(&fd.to_string()));
    // SAFETY: reclaim the deliberately leaked lease fd.
    drop(unsafe { File::from_raw_fd(fd) });
}

#[test]
fn set_fd_flags_surfaces_a_bad_fd() {
    let err = set_fd_flags(999999, 0).unwrap_err();
    assert_eq!(err.raw_os_error(), Some(libc::EBADF));
}

#[test]
fn interpret_helpers_map_errors_to_their_adopt_arms() {
    // The cloexec/flock failure arms are unreachable on a validated fd
    // (fstat already proved it live), so the mappers are exercised
    // directly — the same pattern as `lock::interpret_lock`.
    let e = interpret_cloexec(7, Err(io::Error::other("x"))).unwrap_err();
    assert!(matches!(e, AdoptError::Cloexec { fd: 7, .. }), "{e}");
    let e = interpret_flock(9, Err(io::Error::other("y"))).unwrap_err();
    assert!(matches!(e, AdoptError::Flock { fd: 9, .. }), "{e}");
    assert!(interpret_cloexec(7, Ok(())).is_ok());
}
