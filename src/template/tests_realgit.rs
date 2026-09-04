//! [`RealGit`] runner tests and the stub-capture delegation check.
//! Split from [`super::tests`] for the per-file line cap.

use super::{GitRunner, RealGit};
use std::io;
use std::path::PathBuf;
use tempfile::TempDir;

#[test]
fn realgit_default_matches_new() {
    let _ = RealGit::default();
}

#[test]
fn realgit_succeeds_on_valid_command() {
    let holder = TempDir::new().unwrap();
    RealGit::new()
        .run(holder.path(), &["init", "-b", "main"])
        .unwrap();
    assert!(holder.path().join(".git").is_dir());
}

#[test]
fn realgit_returns_error_on_nonzero_exit() {
    let holder = TempDir::new().unwrap();
    // No git repo here, so `git status` exits non-zero. That hits
    // the `!status.success()` branch without needing a missing
    // binary.
    let err = RealGit::new()
        .run(holder.path(), &["status", "--porcelain"])
        .unwrap_err();
    let msg = err.to_string();
    assert!(msg.contains("exited with"), "unexpected: {msg}");
}

#[test]
fn realgit_returns_error_when_binary_missing() {
    let holder = TempDir::new().unwrap();
    let git = RealGit {
        bin: PathBuf::from("/no/such/litany-test-git"),
    };
    let err = git.run(holder.path(), &["init"]).unwrap_err();
    assert_eq!(err.kind(), io::ErrorKind::NotFound);
}

#[test]
fn realgit_run_capture_returns_stdout() {
    // `git --version` prints a line to stdout that RealGit trims.
    let holder = TempDir::new().unwrap();
    let out = RealGit::new()
        .run_capture(holder.path(), &["--version"])
        .unwrap();
    assert!(out.starts_with("git "), "unexpected: {out:?}");
}

#[test]
fn realgit_run_capture_bytes_is_untrimmed() {
    // The raw-bytes capture is what `search_history` hands the model
    // (ARCH §3.3): `git --version` ends in a newline, and the trimmed
    // string capture drops it. Verbatim means the newline survives.
    let holder = TempDir::new().unwrap();
    let git = RealGit::new();
    let raw = git
        .run_capture_bytes(holder.path(), &["--version"])
        .unwrap();
    assert!(raw.ends_with(b"\n"), "unexpected: {raw:?}");
    let trimmed = git.run_capture(holder.path(), &["--version"]).unwrap();
    assert_eq!(String::from_utf8_lossy(&raw).trim(), trimmed);
}

/// A runner that answers only [`GitRunner::run_capture`] — the shape
/// every test double in the tree has — proving the trait's default
/// `run_capture_bytes` delegates to it rather than demanding a second
/// answer of them.
struct CaptureOnly;
impl GitRunner for CaptureOnly {
    fn run(&self, dest: &std::path::Path, args: &[&str]) -> io::Result<()> {
        self.run_capture(dest, args).map(|_| ())
    }
    fn run_capture(&self, _dest: &std::path::Path, _args: &[&str]) -> io::Result<String> {
        Ok("answered".to_string())
    }
}

#[test]
fn the_default_capture_bytes_delegates_to_run_capture() {
    CaptureOnly
        .run(std::path::Path::new("."), &["anything"])
        .unwrap();
    let bytes = CaptureOnly
        .run_capture_bytes(std::path::Path::new("."), &["anything"])
        .unwrap();
    assert_eq!(bytes, b"answered".to_vec());
}
