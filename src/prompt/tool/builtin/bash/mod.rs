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
//! The shell runs in its own process group and is ended by the §2.9
//! cascade like any spawned child; both live in [`super::child`], which
//! `python` shares.

use super::child;
use serde::Deserialize;
use std::io::{self, Read, Write};
use std::process::Command;
use std::sync::atomic::AtomicBool;
use std::time::Duration;
use thiserror::Error;

/// Wire shape of the input. `serde(deny_unknown_fields)` so a malformed
/// `tool_use.input` surfaces as [`Error::InvalidJson`] rather than
/// silently dropping fields the model meant to pass.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Input {
    command: String,
}

/// Every way [`run`] can fail. In-band shell failures (the command exits
/// non-zero) are *not* errors here — they propagate via the returned
/// exit code and the captured stderr bytes.
#[derive(Debug, Error)]
pub enum Error {
    /// Stdin handed back bytes that did not parse as the documented
    /// shape — wrong type, missing `command`, or extra fields.
    #[error("invalid input JSON: {0}")]
    InvalidJson(#[source] serde_json::Error),
    /// The harness's stdin pipe failed mid-read.
    #[error("read input from stdin: {0}")]
    StdinRead(#[source] io::Error),
    /// The shell could not be spawned or reaped ([`child::Error`]).
    #[error(transparent)]
    Child(#[from] child::Error),
    /// Writing the captured shell stdout to the harness's stdout failed
    /// — only fires when the executor's pipe is closed before we finish
    /// streaming the buffer through.
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
    child::install_sigterm_handler();
    run_with(stdin, stdout, stderr, "sh", child::sigterm_flag(), child::CASCADE_DEADLINE)
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
    cmd.arg("-c").arg(&input.command);
    let done = child::run(&mut cmd, None, stop, deadline)?;

    stdout.write_all(&done.stdout).map_err(Error::Stdout)?;
    stderr.write_all(&done.stderr).map_err(Error::Stderr)?;
    Ok(done.code)
}

#[cfg(test)]
mod tests;
