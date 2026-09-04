//! Unit tests for [`super::run_with`]. Every error variant and the
//! cancel cascade has its own test so a coverage regression points at
//! the offending path. The production [`super::run`] is exercised via
//! the integration test (`tests/bash_tool.rs`) so the SIGTERM-handler
//! installation does not pollute the unit-test process.

use super::*;
use std::io::Cursor;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use std::sync::atomic::Ordering;
use std::thread;
use std::time::{Duration, Instant};

fn input_for(command: &str) -> Vec<u8> {
    serde_json::json!({ "command": command })
        .to_string()
        .into_bytes()
}

fn run_sh<R: Read>(
    mut stdin: R,
    stop: &AtomicBool,
    deadline_ms: u64,
) -> (Result<i32, Error>, Vec<u8>, Vec<u8>) {
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();
    let deadline = Duration::from_millis(deadline_ms);
    let result = run_with(&mut stdin, &mut stdout, &mut stderr, "sh", stop, deadline);
    (result, stdout, stderr)
}

fn never_stop() -> AtomicBool {
    AtomicBool::new(false)
}

/// Spawn a thread that flips `stop` after `delay_ms` so the cascade
/// fires while the main thread is in `run_with`.
fn schedule_stop(stop: Arc<AtomicBool>, delay_ms: u64) -> thread::JoinHandle<()> {
    thread::spawn(move || {
        thread::sleep(Duration::from_millis(delay_ms));
        stop.store(true, Ordering::SeqCst);
    })
}

#[test]
fn happy_path_returns_zero_and_stdout_bytes() {
    let (code, out, err) = run_sh(Cursor::new(input_for("printf hello")), &never_stop(), 100);
    assert_eq!(code.unwrap(), 0);
    assert_eq!(out, b"hello");
    assert!(err.is_empty(), "stderr was {err:?}");
}

#[test]
fn nonzero_exit_propagated_with_stderr_separated() {
    let (code, out, err) = run_sh(
        Cursor::new(input_for("printf out; printf warn 1>&2; false")),
        &never_stop(),
        100,
    );
    assert_eq!(code.unwrap(), 1);
    assert_eq!(out, b"out");
    assert_eq!(err, b"warn");
}

#[test]
fn arbitrary_exit_code_round_trips() {
    let (code, _, _) = run_sh(Cursor::new(input_for("exit 42")), &never_stop(), 100);
    assert_eq!(code.unwrap(), 42);
}

#[test]
fn invalid_json_input_surfaces_invalid_json() {
    let (code, _, _) = run_sh(Cursor::new(b"not json".to_vec()), &never_stop(), 100);
    assert!(matches!(code, Err(Error::InvalidJson(_))), "{code:?}");
}

#[test]
fn missing_command_field_surfaces_invalid_json() {
    let (code, _, _) = run_sh(
        Cursor::new(br#"{"other": "x"}"#.to_vec()),
        &never_stop(),
        100,
    );
    assert!(matches!(code, Err(Error::InvalidJson(_))), "{code:?}");
}

#[test]
fn extra_fields_rejected_by_deny_unknown_fields() {
    let (code, _, _) = run_sh(
        Cursor::new(br#"{"command": "true", "extra": 1}"#.to_vec()),
        &never_stop(),
        100,
    );
    assert!(matches!(code, Err(Error::InvalidJson(_))), "{code:?}");
}

#[test]
fn stdin_read_error_surfaces_stdin_read() {
    /// A `Read` impl that always errors — exercises the stdin-read
    /// branch without a closed fd.
    struct BrokenReader;
    impl Read for BrokenReader {
        fn read(&mut self, _: &mut [u8]) -> io::Result<usize> {
            Err(io::Error::other("stdin pipe broken"))
        }
    }
    let (code, _, _) = run_sh(BrokenReader, &never_stop(), 100);
    assert!(matches!(code, Err(Error::StdinRead(_))), "{code:?}");
}

#[test]
fn missing_shell_surfaces_spawn() {
    let mut stdin = Cursor::new(input_for("true"));
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();
    let err = run_with(
        &mut stdin,
        &mut stdout,
        &mut stderr,
        "/no/such/shell-binary",
        &never_stop(),
        Duration::from_millis(100),
    )
    .unwrap_err();
    assert!(matches!(err, Error::Child(child::Error::Spawn(_))), "{err}");
}

/// `Write` that errors on every call. Used by the stdout / stderr
/// failure-mode tests below; sharing one impl keeps both branches
/// exercised through the same shape of broken pipe.
struct BrokenWriter;
impl Write for BrokenWriter {
    fn write(&mut self, _: &[u8]) -> io::Result<usize> {
        Err(io::Error::other("pipe closed"))
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

#[test]
fn stdout_write_error_surfaces_stdout() {
    let mut stdin = Cursor::new(input_for("printf has-output"));
    let mut stdout = BrokenWriter;
    let mut stderr = Vec::new();
    let err = run_with(
        &mut stdin,
        &mut stdout,
        &mut stderr,
        "sh",
        &never_stop(),
        Duration::from_millis(100),
    )
    .unwrap_err();
    assert!(matches!(err, Error::Stdout(_)), "{err}");
}

#[test]
fn stderr_write_error_surfaces_stderr() {
    let mut stdin = Cursor::new(input_for("printf out; printf warn 1>&2"));
    let mut stdout = Vec::new();
    let mut stderr = BrokenWriter;
    let err = run_with(
        &mut stdin,
        &mut stdout,
        &mut stderr,
        "sh",
        &never_stop(),
        Duration::from_millis(100),
    )
    .unwrap_err();
    assert!(matches!(err, Error::Stderr(_)), "{err}");
}

#[test]
fn cascade_kills_child_when_stop_flips() {
    let stop = Arc::new(AtomicBool::new(false));
    let flipper = schedule_stop(stop.clone(), 50);
    let started = Instant::now();
    let (code, _, _) = run_sh(Cursor::new(input_for("sleep 30")), &stop, 100);
    let elapsed = started.elapsed();
    flipper.join().unwrap();
    assert!(elapsed < Duration::from_secs(5), "elapsed: {elapsed:?}");
    let code = code.unwrap();
    // SIGTERM-killed sleep reports `128 + 15 = 143`; on systems where
    // sh traps and re-raises differently, SIGKILL (`128 + 9 = 137`)
    // is also a legitimate cascade outcome.
    assert!(
        code == 128 + libc::SIGTERM || code == 128 + libc::SIGKILL,
        "code: {code}"
    );
}

#[test]
fn cascade_kills_descendant_subprocess_tree() {
    let stop = Arc::new(AtomicBool::new(false));
    let flipper = schedule_stop(stop.clone(), 50);
    let started = Instant::now();
    let (code, _, _) = run_sh(Cursor::new(input_for("sleep 30 & wait $!")), &stop, 100);
    let elapsed = started.elapsed();
    flipper.join().unwrap();
    // The `wait $!` would block for 30s if the descendant survived
    // the cascade; this is what proves the process-group kill
    // reached the grandchild.
    assert!(
        elapsed < Duration::from_secs(5),
        "tree took too long to tear down: {elapsed:?}",
    );
    code.unwrap(); // Whichever signal surfaces, it's a clean tree-kill.
}

#[test]
fn cascade_escalates_to_sigkill_when_sigterm_is_trapped() {
    // The shell traps SIGTERM and ignores it; only the SIGKILL leg
    // of the cascade can free us.
    let stop = Arc::new(AtomicBool::new(false));
    let flipper = schedule_stop(stop.clone(), 50);
    let started = Instant::now();
    let (code, _, _) = run_sh(Cursor::new(input_for("trap '' TERM; sleep 30")), &stop, 150);
    let elapsed = started.elapsed();
    flipper.join().unwrap();
    assert!(elapsed < Duration::from_secs(5), "elapsed: {elapsed:?}");
    assert_eq!(code.unwrap(), 128 + libc::SIGKILL);
}

#[test]
fn broken_writer_flush_is_inert() {
    // The `Write` trait requires `flush`; the impl is a no-op so the
    // stdout / stderr failure-path tests can re-use one writer.
    // Explicit assertion keeps the impl's body covered.
    BrokenWriter.flush().unwrap();
}
