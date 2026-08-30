//! Built-in tools — the in-process implementations behind
//! `litany tool <name>` (ARCH §3.3, §12 v0.3 toolset).
//!
//! Each tool is a pure function over [`Read`]/[`Write`] so unit tests
//! drive it without touching real stdio. The `litany tool` subcommand
//! is a thin shim that locks the process's stdio handles and delegates
//! to [`run`]; the §3.3 stdio contract (stdin = `tool_use.input` JSON,
//! stdout = raw result bytes, exit code = is_error) is enforced here.
//!
//! v0.3 shipped two built-ins (`read_file`, `bash`); v0.4 Phase 2 adds
//! [`dispatch`] (the subagent-spawning tool, ARCH §2.5), and the inbox
//! substrate adds [`message`] (deposit content into an existing agent's
//! inbox, ARCH §2.11). [`load_skill`] realizes Body-on-demand (§3.3):
//! it copies a pooled skill directory into the worktree at
//! `skills/<name>/`, committed with the tool result so the next
//! assembly composes it. [`cd`] moves the calling agent's working
//! directory for every later tool call (§3.3 *Working directory*),
//! storing it as the agent's own mark. A dispatch returns the child's
//! address immediately and never blocks; a message deposits
//! synchronously and returns `{status: deposited}`; a load_skill copies
//! and returns `{status: loaded|already_loaded}`; a cd returns
//! `{cwd: <absolute path>}`. [`apply_patch`] is the structured edit
//! path (§3.3 *The patch tool*): one atomic multi-file envelope,
//! located through the matching ladder, applied all-or-nothing.
//! All derive the calling agent's
//! identity from `LITANY_CONV_BRANCH` (§3.3), never from model input.
//! Adding a new one is a match arm in [`run`] plus a sibling module.

use std::io::{Read, Write};
use std::path::Path;
use thiserror::Error;

pub mod apply_patch;
pub mod bash;
pub mod cd;
pub mod compaction;
pub mod dispatch;
pub mod load_skill;
pub mod message;
pub mod read_file;

/// Built-in tool name: atomic multi-file structured edit (§3.3 *The
/// patch tool*).
const APPLY_PATCH: &str = "apply_patch";
/// Built-in tool name: run a shell command (§3.3).
const BASH: &str = "bash";
/// Built-in tool name: move the calling agent's working directory (§3.3
/// *Working directory*).
const CD: &str = "cd";
/// Built-in tool name: spawn a subagent (§2.5).
const DISPATCH: &str = "dispatch";
/// Built-in tool name: copy a pooled skill body into the worktree (§3.3
/// Body-on-demand).
const LOAD_SKILL: &str = "load_skill";
/// Built-in tool name: deposit into an existing agent's inbox (§2.11).
const MESSAGE: &str = "message";
/// Built-in tool name: read a file's bytes (§3.3).
const READ_FILE: &str = "read_file";

/// The closed set of built-in tool names `litany tool <name>` answers to,
/// sorted — the one list behind both the [`Error::Unknown`] decline and
/// the `<NAME>` argument's CLI help (PRINCIPLES single source of truth).
/// The compactor pair (`write_summary` / `mark_for_deletion`) is
/// deliberately absent: it is injected for the compactor role alone
/// (§2.7), never a name a general agent or an operator elects, so it is
/// routed but not advertised.
pub const NAMES: [&str; 7] = [
    APPLY_PATCH,
    BASH,
    CD,
    DISPATCH,
    LOAD_SKILL,
    MESSAGE,
    READ_FILE,
];

/// [`NAMES`] rendered for a human: the pool named in the unknown-tool
/// decline and in `litany tool --help`, in the same voice `load_skill`
/// names its own pool with (§3.3 "declined … naming the available pool").
pub fn pool() -> String {
    NAMES.join(", ")
}

/// Reasons [`run`] can fail. Each in-process tool surfaces its own
/// error variant; an unknown tool name is the dispatcher-level case.
#[derive(Debug, Error)]
pub enum Error {
    /// The litany binary was invoked as `litany tool <name>` for a
    /// `<name>` that isn't a built-in. The harness only routes here
    /// after external resolution misses (§3.3), so this is "no tool
    /// of that name exists at all". The decline names the available pool
    /// ([`NAMES`]) — the same idiom `load_skill` declines an unknown skill
    /// with (§3.3), so the model (or the operator typing the subcommand by
    /// hand) is told what it *could* have said.
    #[error("unknown built-in tool: {0:?}; available: {available}", available = pool())]
    Unknown(String),
    /// `read_file` failed; carries the inner reason for the operator's
    /// `eprintln!`. The §3.3 stdio contract concats stderr after
    /// stdout into `tool_result.content` when exit code is non-zero,
    /// so the message reaches the model verbatim.
    #[error(transparent)]
    ReadFile(#[from] read_file::Error),
    /// `apply_patch` refused or failed (bad input JSON, an envelope that
    /// did not parse, stale or ambiguous context, a write fault, per
    /// [`apply_patch::Error`], §3.3 *The patch tool*). A refused patch
    /// is a decline the model reads as an `is_error` `tool_result`
    /// carrying the exact reason; nothing was written. Same
    /// stderr-concat contract as the other arms.
    #[error(transparent)]
    ApplyPatch(#[from] apply_patch::Error),
    /// `bash` failed at the harness layer (bad input JSON, spawn
    /// failure, broken pipe, etc.). In-band shell failures — the
    /// command ran and exited non-zero — are *not* this variant; they
    /// flow through the returned exit code.
    #[error(transparent)]
    Bash(#[from] bash::Error),
    /// `dispatch` failed (bad input JSON, missing role / soul,
    /// `litany dispatch <role>` exit non-zero, etc., per
    /// [`dispatch::Error`]). The §3.3 stdio contract concats stderr
    /// after stdout so the agent sees the failure verbatim.
    #[error(transparent)]
    Dispatch(#[from] dispatch::Error),
    /// `message` failed (bad input JSON, missing env, `litany message`
    /// exit non-zero, etc., per [`message::Error`]). Same stderr-concat
    /// contract as the other arms.
    #[error(transparent)]
    Message(#[from] message::Error),
    /// `cd` failed (bad input JSON, missing env, a path that names no
    /// directory, a mark that could not be stored, per [`cd::Error`],
    /// ARCH §3.3 *Working directory*). A path the agent cannot move to
    /// is a decline the model reads as an `is_error` `tool_result`; the
    /// agent stays where it was. Same stderr-concat contract.
    #[error(transparent)]
    Cd(#[from] cd::Error),
    /// `load_skill` failed (bad input JSON, missing env, unknown skill,
    /// copy failure, etc., per [`load_skill::Error`]). An unknown skill
    /// is a decline that reaches the model as an `is_error` `tool_result`
    /// naming the available pool (§3.3). Same stderr-concat contract.
    #[error(transparent)]
    LoadSkill(#[from] load_skill::Error),
    /// A compactor tool (`write_summary` / `mark_for_deletion`) failed
    /// (bad input JSON, missing env, deletion-only decline, etc., per
    /// [`compaction::Error`], ARCH §2.7). Same stderr-concat contract.
    #[error(transparent)]
    Compaction(#[from] compaction::Error),
}

/// Dispatch one in-process tool call. `name` is the tool name as the
/// model spelled it (and as the harness passed via `litany tool
/// <name>`); `driver_target` is the binding-injected re-entry path
/// (`cmd::Fx::driver_target`, §2.11) the `dispatch` and `message`
/// built-ins go back through the front door with; `stdin` carries the
/// `tool_use.input` JSON; `stdout`
/// receives the bytes the executor will surface as
/// `tool_result.content` on success; `stderr` receives the bytes that
/// — per §3.3 — concatenate after stdout when the exit code is
/// non-zero. The returned `i32` is the desired process exit code:
/// `read_file` always returns 0 on success and lets [`Error`] carry
/// failure; `bash` propagates the shell's own exit code so a non-zero
/// command can flow through without being misclassified as a harness
/// fault.
// `#[rustfmt::skip]` keeps the `run_with` tail call on one line: exploded
// across arg lines, tarpaulin's llvm engine mis-attributes the argument
// lines as uncovered (a known multi-line-call quirk), and every line here
// is exercised by the routing tests in [`tests`].
#[rustfmt::skip]
pub fn run<R: Read, W: Write, E: Write>(
    name: &str,
    driver_target: &Path,
    stdin: &mut R,
    stdout: &mut W,
    stderr: &mut E,
) -> Result<i32, Error> {
    let spawner = dispatch::SubprocessSpawner::with_exe(driver_target.to_path_buf());
    let sender = message::SubprocessSender::with_exe(driver_target.to_path_buf());
    run_with(name, stdin, stdout, stderr, &dispatch::ProcessEnv, &spawner, &sender)
}

/// Same as [`run`] but with the `dispatch`-tool dependencies (env
/// lookup + subprocess spawner) injected. Production wires these to
/// [`dispatch::ProcessEnv`] + [`dispatch::SubprocessSpawner`] via
/// [`run`]; tests inject stubs to exercise the dispatch arm without
/// real subprocess fan-out.
pub fn run_with<R: Read, W: Write, E: Write>(
    name: &str,
    stdin: &mut R,
    stdout: &mut W,
    stderr: &mut E,
    env: &dyn dispatch::EnvLookup,
    spawner: &dyn dispatch::Spawner,
    sender: &dyn message::Sender,
) -> Result<i32, Error> {
    if name == READ_FILE {
        return read_file::run(stdin, stdout)
            .map(|()| 0)
            .map_err(Error::ReadFile);
    }
    if name == APPLY_PATCH {
        return apply_patch::run(stdin, stdout)
            .map(|()| 0)
            .map_err(Error::ApplyPatch);
    }
    if name == BASH {
        return bash::run(stdin, stdout, stderr).map_err(Error::Bash);
    }
    if name == DISPATCH {
        return dispatch::run(stdin, stdout, env, spawner)
            .map(|()| 0)
            .map_err(Error::Dispatch);
    }
    if name == MESSAGE {
        return message::run(stdin, stdout, env, sender)
            .map(|()| 0)
            .map_err(Error::Message);
    }
    if name == CD {
        return cd::run(stdin, stdout, env).map(|()| 0).map_err(Error::Cd);
    }
    if name == LOAD_SKILL {
        return load_skill::run(stdin, stdout, env)
            .map(|()| 0)
            .map_err(Error::LoadSkill);
    }
    // The compactor toolset (§2.7), built into the primitive: available to
    // the compactor role's injected toolset, not any `providers.yaml` list.
    if name == crate::prompt::compactor::tools::WRITE_SUMMARY {
        return compaction::run_write_summary(stdin, stdout, env)
            .map(|()| 0)
            .map_err(Error::Compaction);
    }
    if name == crate::prompt::compactor::tools::MARK_FOR_DELETION {
        return compaction::run_mark_for_deletion(stdin, stdout, env)
            .map(|()| 0)
            .map_err(Error::Compaction);
    }
    Err(Error::Unknown(name.to_string()))
}

#[cfg(test)]
mod tests;
