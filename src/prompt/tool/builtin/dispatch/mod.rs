//! `dispatch` built-in (ARCH §2.5, §3.3, §3.4).
//!
//! Stdin is the `tool_use.input` block as JSON: `{ "role": <string>,
//! "goal": <string>, "name": <string|absent> }`. The conversation context (which workspace,
//! which calling branch) arrives via the `LITANY_CONV_REPO` and
//! `LITANY_CONV_BRANCH` env vars the executor sets per ARCH §3.3 — it
//! is not in the model-facing input schema because the model does not
//! pick which conversation it is part of.
//!
//! The tool starts the child through the §3.4 control plane —
//! `litany dispatch <role> <repo> <branch> --goal <goal>` — rather than
//! forking in-process. That CLI does the whole dispatch primitive: fork
//! the child branch + dispatch commit, then deposit the dispatch message
//! through the front door so the child's driver (`litany advance`, §6)
//! starts nominally (ARCH §2.5 — fork plus front door, never a spawn).
//! It prints the child's id on stdout; the dispatch tool captures that
//! address and re-emits it on its own stdout as the `tool_result`
//! payload `{"status":"in_progress","handle":"<child-id>"}` — the child
//! runs asynchronously and its result returns later as a deposit into
//! this agent's inbox (§2.5, §2.6 — the dispatch is the child's first
//! prompt), never through this tool's return.

use serde::{Deserialize, Serialize};
use std::ffi::OsString;
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::process::Command;
use thiserror::Error;

/// Wire shape of the input. `serde(deny_unknown_fields)` so a
/// malformed `tool_use.input` surfaces as [`Error::InvalidJson`]
/// rather than silently dropping fields the model meant to pass.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Input {
    role: String,
    goal: String,
    /// The child's display name (ARCH §2.3, §2.11), forwarded as
    /// `litany dispatch --name`. An exposed parameter the schema teaches
    /// (yog bl-aca4): supplied, it is the child's identity in every
    /// surface — `message`-addressable, tree-readable; absent, the verb
    /// mints a valid one-word name, so omission is never an error. A
    /// supplied name that is malformed or already worn is declined by
    /// the verb, so the model sees the refusal verbatim (§3.3) and no
    /// child is created.
    #[serde(default)]
    name: Option<String>,
}

/// Wire shape of the output — the `tool_result.content` payload the
/// agent sees on its next step. `status` is always `in_progress` here
/// (ARCH §2.5 — dispatch returns the child's address immediately and
/// never blocks; the child's terminal result arrives later as a
/// deposit into this agent's inbox, §2.11, not via any polling call);
/// `handle` is the subagent's full hyphenated descent branch
/// (`<parent>-<sub-id>`, ARCH §2.2 / §2.3), which is also its address.
#[derive(Debug, Serialize, PartialEq, Eq)]
struct Output<'a> {
    status: &'a str,
    handle: &'a str,
}
const STATUS_IN_PROGRESS: &str = "in_progress";

/// Every way [`run`] can fail. Each variant prints its own stderr
/// message; per ARCH §3.3 the result envelope carries stderr into
/// `tool_result.content` under its marker, so the model sees the
/// failure verbatim alongside the stated exit code.
#[derive(Debug, Error)]
pub enum Error {
    /// Stdin handed back bytes that did not parse as the documented
    /// `{role, goal}` shape — wrong type, missing field, extra field.
    #[error("invalid input JSON: {0}")]
    InvalidJson(#[source] serde_json::Error),
    /// The harness's stdin pipe failed mid-read.
    #[error("read input from stdin: {0}")]
    StdinRead(#[source] io::Error),
    /// Required env var (`LITANY_CONV_REPO` / `LITANY_CONV_BRANCH`)
    /// not set. Production callers always set these; the variant
    /// exists so a hand-invoked `litany tool dispatch` outside a
    /// step gets a clear message instead of a confusing soul-read
    /// failure.
    #[error("missing env var {0:?} (set by the harness per ARCH §3.3)")]
    MissingEnv(&'static str),
    /// The open-set role verdict (§4.3): unlisted role, missing soul,
    /// malformed `providers.yaml`, or a governing-config derivation
    /// failure. Rendered by the single-home
    /// [`crate::prompt::role::validate::Invalid`] itself rather than
    /// restated here — one voice, whether the refusal reaches a model
    /// through this tool or an operator through `litany dispatch`.
    #[error(transparent)]
    Role(#[from] crate::prompt::role::validate::Invalid),
    /// `litany dispatch <role>` failed to spawn (binary missing,
    /// fork limits, etc.).
    #[error("spawn litany dispatch {role:?}: {source}")]
    Spawn {
        role: String,
        #[source]
        source: io::Error,
    },
    /// `litany dispatch <role>` exited non-zero. The subprocess's
    /// stderr is folded into the message so the failure reaches the
    /// agent verbatim.
    #[error("litany dispatch {role:?} failed (exit {exit}): {stderr}")]
    DispatchExit {
        role: String,
        exit: i32,
        stderr: String,
    },
    /// `litany dispatch <role>` exited 0 but printed no branch name —
    /// indicates a CLI contract regression (Phase 1 always prints the
    /// sub-branch on stdout for the worker role).
    #[error("litany dispatch {role:?} produced no handle on stdout")]
    EmptyHandle { role: String },
    /// Writing the JSON output to stdout failed.
    #[error("write to stdout: {0}")]
    Write(#[source] io::Error),
}

/// Trait for invoking `litany dispatch <role>`. Production wires
/// [`SubprocessSpawner`]; tests inject a stub that fabricates the
/// stdout (sub-branch name) without spawning a real subprocess.
pub trait Spawner {
    /// Run `litany dispatch <role> <repo> <branch> --goal <goal>
    /// [--name <name>]` and return the captured stdout (which Phase 1's
    /// CLI sets to the new sub-branch name on the worker role).
    fn dispatch(
        &self,
        role: &str,
        repo: &Path,
        branch: &str,
        goal: &str,
        name: Option<&str>,
    ) -> Result<DispatchOutput, io::Error>;
}

/// Captured outcome of `litany dispatch <role>`. Mirrors the
/// `Captured` shape used elsewhere in the executor but stays local to
/// the dispatch tool because the contract is tighter — we always read
/// stdout as text and we report exit non-zero as a typed [`Error`].
#[derive(Debug)]
pub struct DispatchOutput {
    pub stdout: String,
    pub stderr: String,
    pub exit: i32,
}

/// Production [`Spawner`] — re-enters a `litany` binary as
/// `litany dispatch <role>`. The dispatch tool itself is `litany tool
/// dispatch` running in-process; re-entering the same binary keeps
/// the §3.4 "everyone uses the front door" rule intact. The exe path
/// is the binding-injected driver target (`cmd::Fx::driver_target`,
/// §2.11 "the driver target is injected at the binding, not resolved by
/// name") — never `current_exe`, which under a linked host names the
/// host binary, and the host carries no `dispatch` verb of its own.
pub struct SubprocessSpawner {
    exe: PathBuf,
}

impl SubprocessSpawner {
    /// Re-enter `exe` — the injected driver target in production, a
    /// `true`/`false` stand-in in tests that exercise the wrapper
    /// without spawning the real `litany`.
    pub fn with_exe(exe: PathBuf) -> Self {
        Self { exe }
    }
}

impl Spawner for SubprocessSpawner {
    fn dispatch(
        &self,
        role: &str,
        repo: &Path,
        branch: &str,
        goal: &str,
        name: Option<&str>,
    ) -> Result<DispatchOutput, io::Error> {
        let mut cmd = Command::new(&self.exe);
        cmd.args(["dispatch", role])
            .arg(repo)
            .arg(branch)
            .args(["--goal", goal]);
        if let Some(name) = name {
            cmd.args(["--name", name]);
        }
        let out = cmd.output()?;
        Ok(DispatchOutput {
            stdout: String::from_utf8_lossy(&out.stdout).into_owned(),
            stderr: String::from_utf8_lossy(&out.stderr).into_owned(),
            exit: out.status.code().unwrap_or(-1),
        })
    }
}

/// Trait for env-var lookup. Production reads `std::env::var`; tests
/// inject a fixed map so the conv-repo / conv-branch values are not
/// dependent on global process state (which `cargo test` runs in
/// parallel with).
pub trait EnvLookup {
    fn get(&self, key: &str) -> Option<OsString>;
}

/// Production [`EnvLookup`] — reads the live process environment.
pub struct ProcessEnv;

impl EnvLookup for ProcessEnv {
    fn get(&self, key: &str) -> Option<OsString> {
        std::env::var_os(key)
    }
}

/// Pure entry point: parse stdin, validate, spawn through `dispatcher`,
/// write the `{status, handle}` JSON to `stdout`. The `litany tool
/// dispatch` shim wires this to the live process's stdio plus
/// [`ProcessEnv`] + [`SubprocessSpawner`].
pub fn run<R: Read, W: Write>(
    stdin: &mut R,
    stdout: &mut W,
    env: &dyn EnvLookup,
    dispatcher: &dyn Spawner,
) -> Result<(), Error> {
    let mut buf = Vec::new();
    stdin.read_to_end(&mut buf).map_err(Error::StdinRead)?;
    let input: Input = serde_json::from_slice(&buf).map_err(Error::InvalidJson)?;

    let repo = require_env(env, super::super::ENV_CONV_REPO)?;
    let branch = require_env(env, super::super::ENV_CONV_BRANCH)?;
    let repo_path = PathBuf::from(repo);
    let branch_str = branch
        .into_string()
        .map_err(|_| Error::MissingEnv(super::super::ENV_CONV_BRANCH))?;

    // Open-set validity (§4.3), the single home shared with the CLI:
    // role listed in the governing config commit + soul present. The `?`
    // projects an `Invalid` verdict onto this tool's `Error` (below).
    // No fork point: the `dispatch` tool's input is `{role, goal}`, so a
    // model-issued dispatch always forks off its own branch (§2.5).
    crate::prompt::role::validate::validate(
        &repo_path,
        &branch_str,
        None,
        &input.role,
        &crate::template::RealGit::new(),
    )?;

    let captured = dispatcher
        .dispatch(
            &input.role,
            &repo_path,
            &branch_str,
            &input.goal,
            input.name.as_deref(),
        )
        .map_err(|source| Error::Spawn {
            role: input.role.clone(),
            source,
        })?;
    if captured.exit != 0 {
        return Err(Error::DispatchExit {
            role: input.role,
            exit: captured.exit,
            stderr: captured.stderr,
        });
    }
    let handle = captured.stdout.trim();
    if handle.is_empty() {
        return Err(Error::EmptyHandle { role: input.role });
    }

    let payload = Output {
        status: STATUS_IN_PROGRESS,
        handle,
    };
    let bytes = serde_json::to_vec(&payload).expect("Output is always serializable");
    stdout.write_all(&bytes).map_err(Error::Write)
}

fn require_env(env: &dyn EnvLookup, key: &'static str) -> Result<OsString, Error> {
    env.get(key).ok_or(Error::MissingEnv(key))
}

#[cfg(test)]
mod tests;
