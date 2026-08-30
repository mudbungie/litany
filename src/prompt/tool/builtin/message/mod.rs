//! `message` built-in (ARCH §2.11, §3.3, §3.4).
//!
//! Stdin is the `tool_use.input` block as JSON: `{ "agent": <string>,
//! "content": <string> }`. The workspace and the sender arrive via the
//! `LITANY_CONV_REPO` and `LITANY_CONV_BRANCH` env vars the executor
//! sets per §3.3 — the sender is thus **harness-derived, never
//! model-supplied**, so an agent cannot forge provenance (§2.11).
//!
//! The tool deposits through the §3.4 control plane — `litany message
//! <workspace> <agent> <content>` — rather than writing the inbox file
//! in-process, the same front-door discipline the `dispatch` built-in
//! follows. The calling agent's id is forwarded to the verb as the
//! `LITANY_CONV_BRANCH` of the spawned process, so the verb resolves the
//! deposit's `<sender>` to it. Deposit is synchronous and returns
//! `{"status":"deposited"}`: it is not a dispatch, creates no branch,
//! and returns no child address (§2.11).

use serde::{Deserialize, Serialize};
use std::ffi::OsString;
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::process::Command;
use thiserror::Error;

use super::super::{ENV_CONV_BRANCH, ENV_CONV_REPO};
use super::dispatch::EnvLookup;

/// Wire shape of the input. `deny_unknown_fields` so a model cannot
/// smuggle a `from:`/sender field — provenance is env-derived only.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Input {
    agent: String,
    content: String,
}

/// Wire shape of the output — the `tool_result.content` payload. Deposit
/// always yields `deposited` on success (§2.11).
#[derive(Debug, Serialize, PartialEq, Eq)]
struct Output<'a> {
    status: &'a str,
}
const STATUS_DEPOSITED: &str = "deposited";

/// Every way [`run`] can fail. Each variant prints its own stderr
/// message; per §3.3 stderr concatenates after stdout into
/// `tool_result.content` when exit is non-zero, so the model sees the
/// failure verbatim.
#[derive(Debug, Error)]
pub enum Error {
    #[error("invalid input JSON: {0}")]
    InvalidJson(#[source] serde_json::Error),
    #[error("read input from stdin: {0}")]
    StdinRead(#[source] io::Error),
    #[error("missing env var {0:?} (set by the harness per ARCH §3.3)")]
    MissingEnv(&'static str),
    #[error("spawn litany message: {0}")]
    Spawn(#[source] io::Error),
    #[error("litany message failed (exit {exit}): {stderr}")]
    MessageExit { exit: i32, stderr: String },
    #[error("write to stdout: {0}")]
    Write(#[source] io::Error),
}

/// Captured outcome of `litany message`. Mirrors the dispatch tool's
/// `DispatchOutput` — we always report exit non-zero as a typed error.
#[derive(Debug)]
pub struct SendOutput {
    pub stderr: String,
    pub exit: i32,
}

/// Trait for invoking `litany message`. Production wires
/// [`SubprocessSender`]; tests inject a stub that records the deposit
/// without spawning a subprocess.
pub trait Sender {
    /// Run `litany message <workspace> <agent> <content>` with
    /// `LITANY_CONV_BRANCH=<sender>` so the verb attributes the deposit
    /// to the calling agent.
    fn send(
        &self,
        workspace: &Path,
        agent: &str,
        content: &str,
        sender: &str,
    ) -> Result<SendOutput, io::Error>;
}

/// Production [`Sender`] — re-enters the `litany` command surface as
/// `litany message`. The exe is the binding-injected driver target
/// (`cmd::Fx::driver_target`, §2.11) — never `current_exe`, which under
/// a linked host names the host binary.
pub struct SubprocessSender {
    exe: PathBuf,
}

impl SubprocessSender {
    /// Re-enter `exe` — the injected driver target in production, a
    /// stand-in in tests that avoid spawning the real `litany`.
    pub fn with_exe(exe: PathBuf) -> Self {
        Self { exe }
    }
}

impl Sender for SubprocessSender {
    fn send(
        &self,
        workspace: &Path,
        agent: &str,
        content: &str,
        sender: &str,
    ) -> Result<SendOutput, io::Error> {
        let out = Command::new(&self.exe)
            .arg("message")
            .arg(workspace)
            .arg(agent)
            .arg(content)
            .env(ENV_CONV_BRANCH, sender)
            .output()?;
        Ok(SendOutput {
            stderr: String::from_utf8_lossy(&out.stderr).into_owned(),
            exit: out.status.code().unwrap_or(-1),
        })
    }
}

/// Pure entry point: parse stdin, read the harness env, deposit through
/// `sender`, write `{status: deposited}` to `stdout`. The `litany tool
/// message` shim wires this to the live process's stdio plus
/// [`super::dispatch::ProcessEnv`] + [`SubprocessSender`].
pub fn run<R: Read, W: Write>(
    stdin: &mut R,
    stdout: &mut W,
    env: &dyn EnvLookup,
    sender_impl: &dyn Sender,
) -> Result<(), Error> {
    let mut buf = Vec::new();
    stdin.read_to_end(&mut buf).map_err(Error::StdinRead)?;
    let input: Input = serde_json::from_slice(&buf).map_err(Error::InvalidJson)?;

    let repo = require_env(env, ENV_CONV_REPO)?;
    let branch = require_env(env, ENV_CONV_BRANCH)?;
    let repo_path = PathBuf::from(repo);
    let sender = branch
        .into_string()
        .map_err(|_| Error::MissingEnv(ENV_CONV_BRANCH))?;

    let captured = sender_impl
        .send(&repo_path, &input.agent, &input.content, &sender)
        .map_err(Error::Spawn)?;
    if captured.exit != 0 {
        return Err(Error::MessageExit {
            exit: captured.exit,
            stderr: captured.stderr,
        });
    }

    let payload = Output {
        status: STATUS_DEPOSITED,
    };
    let bytes = serde_json::to_vec(&payload).expect("Output is always serializable");
    stdout.write_all(&bytes).map_err(Error::Write)
}

fn require_env(env: &dyn EnvLookup, key: &'static str) -> Result<OsString, Error> {
    env.get(key).ok_or(Error::MissingEnv(key))
}

#[cfg(test)]
mod tests;
