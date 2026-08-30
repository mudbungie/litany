//! `bash` built-in (ARCH §3.3, §12 v0.3 toolset).
//!
//! Stdin is the `tool_use.input` block as JSON: `{ "command": <string> }`.
//! Stdout receives the bytes the spawned shell wrote to its stdout;
//! stderr receives the bytes the shell wrote to its stderr. The
//! returned exit code is the shell's own — propagated as-is for normal
//! exits, encoded as `128 + signo` (POSIX) when the shell was killed
//! by a signal.
//!
//! The shell inherits this process's working directory, which the
//! executor pinned to the calling agent's **current** working directory
//! before spawning `litany tool bash` (§3.3 *Working directory*): the
//! agent's worktree by default, or wherever its own [`super::cd`] call
//! last moved it. That inheritance is the whole mechanism: the cwd is
//! resolved once, where the tool call's identity is known. Nothing here
//! re-derives it, and nothing here can change it — a `cd` inside the
//! spawned shell dies with that shell, which is why moving is a tool
//! call and not a command.
//!
//! Side effects therefore ride the tool commit only while the agent is
//! *in* its worktree: the commit stages the worktree (§2.3, §3.3
//! `git add -A`), so a shell writing outside it writes off the record.
//! The `cd` tool definition says so; nothing here enforces it (§3.6
//! defers bounding a tool's authority to the v1.1 sandbox).
//!
//! The shell runs in its own process group so a SIGTERM the harness
//! sends to `litany tool bash` can be forwarded to the entire spawned
//! tree (§2.9 cascade). The internal SIGTERM-then-SIGKILL grace fits
//! inside [`crate::prompt::tool::DEFAULT_TOOL_DEADLINE`] (5s pinned by
//! §3.3) so the executor's outer SIGKILL is never the one that tears
//! the tree down.

use serde::Deserialize;
use std::io::{self, Read, Write};
use std::os::unix::process::{CommandExt, ExitStatusExt};
use std::process::{Command, Stdio};
use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::time::{Duration, Instant};
use thiserror::Error;

/// Internal SIGTERM-then-SIGKILL grace for the spawned shell tree.
/// Sized to comfortably fit inside the 5s outer deadline pinned by
/// ARCH §3.3, with headroom for the executor's polling cadence.
const CASCADE_DEADLINE: Duration = Duration::from_millis(2000);

/// Cadence for polling the child's wait status alongside the stop
/// flag. Small enough that a cancel feels instant; large enough that
/// idle wait time costs nothing measurable.
const POLL_INTERVAL: Duration = Duration::from_millis(20);

/// Wire shape of the input. `serde(deny_unknown_fields)` so a
/// malformed `tool_use.input` surfaces as [`Error::InvalidJson`]
/// rather than silently dropping fields the model meant to pass.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Input {
    command: String,
}

/// Every way [`run`] can fail. In-band shell failures (the command
/// exits non-zero) are *not* errors here — they propagate via the
/// returned exit code and the captured stderr bytes.
#[derive(Debug, Error)]
pub enum Error {
    /// Stdin handed back bytes that did not parse as the documented
    /// shape — wrong type, missing `command`, or extra fields.
    #[error("invalid input JSON: {0}")]
    InvalidJson(#[source] serde_json::Error),
    /// The harness's stdin pipe failed mid-read.
    #[error("read input from stdin: {0}")]
    StdinRead(#[source] io::Error),
    /// `Command::spawn` failed (shell binary missing, fork limits,
    /// permission, etc.).
    #[error("spawn shell: {0}")]
    Spawn(#[source] io::Error),
    /// `Child::wait` failed after the child was already running. Rare
    /// — the kernel almost always reaps cleanly — but distinct from
    /// spawn so the operator knows which side broke.
    #[error("wait shell: {0}")]
    Wait(#[source] io::Error),
    /// Writing the captured shell stdout to the harness's stdout
    /// failed — only fires when the executor's pipe is closed before
    /// we finish streaming the buffer through.
    #[error("write to stdout: {0}")]
    Stdout(#[source] io::Error),
    /// Same as [`Error::Stdout`] but for the stderr stream.
    #[error("write to stderr: {0}")]
    Stderr(#[source] io::Error),
}

/// Production entry point invoked by `litany tool bash`. Installs the
/// SIGTERM forwarder once per process and delegates to [`run_with`].
#[rustfmt::skip]
pub fn run<R: Read, W: Write, E: Write>(
    stdin: &mut R, stdout: &mut W, stderr: &mut E,
) -> Result<i32, Error> {
    install_sigterm_handler();
    run_with(stdin, stdout, stderr, "sh", sigterm_flag(), CASCADE_DEADLINE)
}

/// Test-facing entry point. Lets the cascade be exercised with a
/// caller-owned stop flag and a sub-second deadline, and lets the
/// spawn-failure path be exercised by injecting a missing shell.
#[doc(hidden)]
pub(crate) fn run_with<R: Read, W: Write, E: Write>(
    stdin: &mut R,
    stdout: &mut W,
    stderr: &mut E,
    shell: &str,
    stop: &AtomicBool,
    deadline: Duration,
) -> Result<i32, Error> {
    let mut buf = Vec::new();
    stdin.read_to_end(&mut buf).map_err(Error::StdinRead)?;
    let input: Input = serde_json::from_slice(&buf).map_err(Error::InvalidJson)?;

    let mut cmd = Command::new(shell);
    cmd.arg("-c")
        .arg(&input.command)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    // SAFETY: [`enter_own_process_group`] is async-signal-safe (a
    // single `setpgid` syscall) and is the only code executed between
    // fork and exec. Setting the child's pgid in the child itself is
    // the canonical way to win the race: any descendant the spawned
    // shell forks inherits the new pgid, so the cascade's
    // `kill(-pgid, ...)` reaches the entire tree.
    unsafe {
        cmd.pre_exec(enter_own_process_group);
    }
    let mut child = cmd.spawn().map_err(Error::Spawn)?;

    let pgid = child.id() as i32;
    let mut child_stdout = child.stdout.take().expect("piped");
    let mut child_stderr = child.stderr.take().expect("piped");
    let stdout_thread = thread::spawn(move || {
        let mut buf = Vec::new();
        let _ = child_stdout.read_to_end(&mut buf);
        buf
    });
    let stderr_thread = thread::spawn(move || {
        let mut buf = Vec::new();
        let _ = child_stderr.read_to_end(&mut buf);
        buf
    });

    let status = wait_with_cascade(&mut child, pgid, stop, deadline)?;
    let captured_stdout = stdout_thread.join().expect("stdout reader did not panic");
    let captured_stderr = stderr_thread.join().expect("stderr reader did not panic");

    stdout.write_all(&captured_stdout).map_err(Error::Stdout)?;
    stderr.write_all(&captured_stderr).map_err(Error::Stderr)?;

    // Linux always reports either an exit code or a terminating
    // signal — `(None, None)` is unreachable. `unwrap_or` keeps the
    // total function total without inventing a fake third state.
    let exit_code = status
        .code()
        .or_else(|| status.signal().map(|sig| 128 + sig))
        .unwrap_or(1);
    Ok(exit_code)
}

/// The spawned shell's between-fork-and-exec hook: make the child the
/// leader of its own fresh process group so the cascade's group kill
/// reaches every descendant. `setpgid(0, 0)` on a process that already
/// leads its group is an idempotent success, so this is directly
/// callable in-process — which is also how its lines are covered:
/// coverage counters incremented in the forked child die with the
/// `exec`, so only an in-process call can land in the numerator.
fn enter_own_process_group() -> io::Result<()> {
    // SAFETY: `setpgid` with constant zero arguments touches only the
    // calling process's own group membership.
    unsafe {
        libc::setpgid(0, 0);
    }
    Ok(())
}

/// Poll the child's wait status against the cancel flag. On flag,
/// SIGTERM the entire process group, wait `deadline`, then SIGKILL.
///
/// The wait comes *before* the flag read, not after: a running child is
/// then what puts us in the poll interval, which is a property of the
/// child rather than of when a stop happened to land. Read the flag
/// first and this interval is only entered while the flag is still
/// unset — so on a loaded machine, where a stop scheduled milliseconds
/// out is already set by the first pass, the interval is never entered
/// and the 100% floor loses a line on a diff that touched nothing
/// (bl-1c2e). It costs at most one poll interval of stop latency, which
/// is the granularity the loop already promises.
fn wait_with_cascade(
    child: &mut std::process::Child,
    pgid: i32,
    stop: &AtomicBool,
    deadline: Duration,
) -> Result<std::process::ExitStatus, Error> {
    loop {
        if let Some(status) = child.try_wait().map_err(Error::Wait)? {
            return Ok(status);
        }
        thread::sleep(POLL_INTERVAL);
        if stop.load(Ordering::SeqCst) {
            return cascade_terminate(child, pgid, deadline);
        }
    }
}

/// Send SIGTERM to the child's process group, wait `deadline`, then
/// SIGKILL. Returns the kernel-reported exit status.
fn cascade_terminate(
    child: &mut std::process::Child,
    pgid: i32,
    deadline: Duration,
) -> Result<std::process::ExitStatus, Error> {
    // SAFETY: kill on a process group we created via setpgid; signo
    // is a constant. Negative pid addresses the process group.
    unsafe {
        libc::kill(-pgid, libc::SIGTERM);
    }
    let term_until = Instant::now() + deadline;
    while Instant::now() < term_until {
        if let Some(status) = child.try_wait().map_err(Error::Wait)? {
            return Ok(status);
        }
        thread::sleep(POLL_INTERVAL);
    }
    // SAFETY: same as above; SIGKILL is uncatchable so the final wait
    // is bounded by kernel reap latency.
    unsafe {
        libc::kill(-pgid, libc::SIGKILL);
    }
    child.wait().map_err(Error::Wait)
}

/// Process-wide SIGTERM flag. Set by [`on_sigterm`]; read by the poll
/// loop in [`wait_with_cascade`] when the production [`run`] passes
/// `&SIGTERM_FLAG` as the stop input.
static SIGTERM_FLAG: AtomicBool = AtomicBool::new(false);
static HANDLER_INSTALLED: OnceLock<()> = OnceLock::new();

extern "C" fn on_sigterm(_signo: libc::c_int) {
    // Async-signal-safe: a single atomic store is on POSIX's safe
    // list. The poll loop picks it up on the next tick.
    SIGTERM_FLAG.store(true, Ordering::SeqCst);
}

/// Install [`on_sigterm`] once per process. Idempotent — repeated
/// invocations (e.g. from a second built-in added later) are no-ops.
fn install_sigterm_handler() {
    HANDLER_INSTALLED.get_or_init(|| {
        // SAFETY: `on_sigterm` only stores to an `AtomicBool`, which
        // is async-signal-safe. `libc::signal` is the documented way
        // to install a POSIX handler.
        unsafe {
            libc::signal(libc::SIGTERM, on_sigterm as *const () as libc::sighandler_t);
        }
    });
}

/// Borrow of the process-wide flag, exposed so [`run`] can pass it to
/// [`run_with`] without leaking the static through the public API.
fn sigterm_flag() -> &'static AtomicBool {
    &SIGTERM_FLAG
}

#[cfg(test)]
mod tests;
