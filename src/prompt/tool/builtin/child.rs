//! The **spawned child** a built-in runs and the §2.9 cancel cascade
//! that ends it — one home for both, shared by [`super::bash`] (an
//! `sh -c` command) and [`super::python`] (a `python3 -` interpreter).
//!
//! The child runs in its own process group so a SIGTERM the harness
//! sends to `litany tool <name>` can be forwarded to the entire spawned
//! tree (ARCH §2.9 cascade). The internal SIGTERM-then-SIGKILL grace
//! fits inside [`crate::prompt::tool::DEFAULT_TOOL_DEADLINE`] (5s pinned
//! by §3.3) so the executor's outer SIGKILL is never the one that tears
//! the tree down.
//!
//! Both streams are read on threads of their own and the input is
//! written after those readers are running, so a child that writes more
//! than a pipe buffer while its own stdin is still being fed cannot
//! deadlock against us.
//!
//! No wall-clock limit is imposed anywhere here: the executor imposes
//! none (§3.3), and the two bounds that do exist are `litany stop` (the
//! flag this module reads) and the whole-tree budget (§6).

use std::io::{self, Read, Write};
use std::os::unix::process::{CommandExt, ExitStatusExt};
use std::process::{Command, Stdio};
use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::time::{Duration, Instant};
use thiserror::Error;

/// Internal SIGTERM-then-SIGKILL grace for the spawned tree. Sized to
/// comfortably fit inside the 5s outer deadline pinned by ARCH §3.3,
/// with headroom for the executor's polling cadence.
pub(crate) const CASCADE_DEADLINE: Duration = Duration::from_millis(2000);

/// Cadence for polling the child's wait status alongside the stop flag.
/// Small enough that a cancel feels instant; large enough that idle wait
/// time costs nothing measurable.
const POLL_INTERVAL: Duration = Duration::from_millis(20);

/// What the child left behind: its two captured streams and the exit
/// code the built-in returns — the process's own for a normal exit,
/// `128 + signo` (POSIX) when it was killed by a signal.
pub(crate) struct Finished {
    pub(crate) stdout: Vec<u8>,
    pub(crate) stderr: Vec<u8>,
    pub(crate) code: i32,
}

/// Every way running the child can fail at the harness layer. An
/// in-band failure — the child ran and exited non-zero — is *not* one
/// of these; it rides [`Finished::code`].
#[derive(Debug, Error)]
pub(crate) enum Error {
    /// `Command::spawn` failed (binary missing, fork limits, permission).
    /// A [`std::io::ErrorKind::NotFound`] here is a caller-visible fact:
    /// `python` reads it as the missing interpreter and answers in band
    /// (`docs/DESIGN_CODE_EXECUTION.md` §2.4).
    #[error("spawn the tool's child process: {0}")]
    Spawn(#[source] io::Error),
    /// The child's own stdin pipe failed mid-write.
    #[error("write to the child process's stdin: {0}")]
    Stdin(#[source] io::Error),
    /// `Child::wait` failed after the child was already running. Rare —
    /// the kernel almost always reaps cleanly — but distinct from spawn
    /// so the operator knows which side broke.
    #[error("wait for the tool's child process: {0}")]
    Wait(#[source] io::Error),
}

/// Spawn `cmd`, feed it `input` (nothing means `/dev/null`), capture
/// both streams, and wait for it under the cancel cascade.
pub(crate) fn run(
    cmd: &mut Command,
    input: Option<&[u8]>,
    stop: &AtomicBool,
    deadline: Duration,
) -> Result<Finished, Error> {
    cmd.stdin(if input.is_some() {
        Stdio::piped()
    } else {
        Stdio::null()
    })
    .stdout(Stdio::piped())
    .stderr(Stdio::piped());
    // SAFETY: [`enter_own_process_group`] is async-signal-safe (a single
    // `setpgid` syscall) and is the only code executed between fork and
    // exec. Setting the child's pgid in the child itself is the
    // canonical way to win the race: any descendant it forks inherits
    // the new pgid, so the cascade's `kill(-pgid, ...)` reaches the
    // entire tree.
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
    if let Some(bytes) = input {
        let mut pipe = child.stdin.take().expect("piped");
        pipe.write_all(bytes).map_err(Error::Stdin)?;
    }

    let status = wait_with_cascade(&mut child, pgid, stop, deadline)?;
    Ok(Finished {
        stdout: stdout_thread.join().expect("stdout reader did not panic"),
        stderr: stderr_thread.join().expect("stderr reader did not panic"),
        // Linux always reports either an exit code or a terminating
        // signal — `(None, None)` is unreachable. `unwrap_or` keeps the
        // total function total without inventing a fake third state.
        code: status
            .code()
            .or_else(|| status.signal().map(|sig| 128 + sig))
            .unwrap_or(1),
    })
}

/// The spawned child's between-fork-and-exec hook: make it the leader of
/// its own fresh process group so the cascade's group kill reaches every
/// descendant. `setpgid(0, 0)` on a process that already leads its group
/// is an idempotent success, so this is directly callable in-process —
/// which is also how its lines are covered: coverage counters
/// incremented in the forked child die with the `exec`, so only an
/// in-process call can land in the numerator.
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
    // SAFETY: kill on a process group we created via setpgid; signo is a
    // constant. Negative pid addresses the process group.
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
    // SAFETY: same as above; SIGKILL is uncatchable so the final wait is
    // bounded by kernel reap latency.
    unsafe {
        libc::kill(-pgid, libc::SIGKILL);
    }
    child.wait().map_err(Error::Wait)
}

/// Process-wide SIGTERM flag. Set by [`on_sigterm`]; read by the poll
/// loop in [`wait_with_cascade`] when a production entry point passes
/// [`sigterm_flag`] as the stop input.
static SIGTERM_FLAG: AtomicBool = AtomicBool::new(false);
static HANDLER_INSTALLED: OnceLock<()> = OnceLock::new();

extern "C" fn on_sigterm(_signo: libc::c_int) {
    // Async-signal-safe: a single atomic store is on POSIX's safe list.
    // The poll loop picks it up on the next tick.
    SIGTERM_FLAG.store(true, Ordering::SeqCst);
}

/// Install [`on_sigterm`] once per process. Idempotent — a second
/// built-in reaching it in the same process is a no-op.
pub(crate) fn install_sigterm_handler() {
    HANDLER_INSTALLED.get_or_init(|| {
        // SAFETY: `on_sigterm` only stores to an `AtomicBool`, which is
        // async-signal-safe. `libc::signal` is the documented way to
        // install a POSIX handler.
        unsafe {
            libc::signal(libc::SIGTERM, on_sigterm as *const () as libc::sighandler_t);
        }
    });
}

/// Borrow of the process-wide flag, so a production entry point can pass
/// it to [`run`] without leaking the static through the API.
pub(crate) fn sigterm_flag() -> &'static AtomicBool {
    &SIGTERM_FLAG
}

#[cfg(test)]
mod tests;
