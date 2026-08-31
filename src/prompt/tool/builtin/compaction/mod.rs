//! The compactor toolset as built-in tools (ARCH §2.7): `write_summary`
//! and `mark_for_deletion`.
//!
//! These are **built into the primitive, not declared in
//! `providers.yaml`** (§2.7): they are the compactor role's fixed toolset,
//! and giving the compactor no general filesystem write surface is what
//! makes "deletion-only" structural — the worst failure mode is lost
//! information, never corrupted information (§2.7, §2.6 live-branch-wins).
//!
//! Each is the ordinary §3.3 stdio built-in: stdin is the `tool_use.input`
//! JSON, stdout the JSON result, and the calling agent's workspace +
//! branch arrive via `LITANY_CONV_REPO` / `LITANY_CONV_BRANCH`
//! (harness-derived, §3.3). Both act on the compactor's own worktree; the
//! harness commits the worktree side effect with the tool result under the
//! commit-per-side-effect discipline (§2.3, §3.3 — `git add -A`), so a
//! summary write or a staged deletion lands on the compactor branch and
//! travels to the dispatching branch via the compaction landing (§2.6).

use serde::{Deserialize, Serialize};
use std::io::{self, Read, Write};
use std::path::Path;
use thiserror::Error;

use super::super::{ENV_CONV_BRANCH, ENV_CONV_REPO};
use super::dispatch::EnvLookup;
use crate::prompt::compactor::tools;
use crate::template::{GitRunner, RealGit};
use crate::workspace;

/// `write_summary` input: the summary body to write to the next
/// `summary/<NNN>.md` (§2.7).
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct WriteSummaryInput {
    content: String,
}

/// `mark_for_deletion` input: the branch-relative path to nominate for
/// removal (§2.7). Deletion-only structural — this can remove, never
/// write content.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct MarkInput {
    path: String,
}

/// Result payload (`tool_result.content`): `status` plus the
/// worktree-relative `path` acted on.
#[derive(Debug, Serialize, PartialEq, Eq)]
struct Output<'a> {
    status: &'a str,
    path: String,
}

/// Every way the compactor tools can fail. Each prints its own stderr
/// message; per §3.3, stderr concatenates after stdout into
/// `tool_result.content` on a non-zero exit, so the model sees it verbatim.
#[derive(Debug, Error)]
pub enum Error {
    #[error("invalid input JSON: {0}")]
    InvalidJson(#[source] serde_json::Error),
    #[error("read input from stdin: {0}")]
    StdinRead(#[source] io::Error),
    #[error("missing env var {0:?} (set by the harness per ARCH §3.3)")]
    MissingEnv(&'static str),
    #[error("write summary: {0}")]
    WriteSummary(#[source] io::Error),
    #[error("mark for deletion: {0}")]
    Mark(#[source] crate::prompt::Error),
    #[error("write to stdout: {0}")]
    Write(#[source] io::Error),
}

/// `write_summary`: write the next `summary/<NNN>.md` on the compactor's
/// worktree (§2.7). The harness commits it with the tool result.
pub fn run_write_summary<R: Read, W: Write>(
    stdin: &mut R,
    stdout: &mut W,
    env: &dyn EnvLookup,
) -> Result<(), Error> {
    let input: WriteSummaryInput = read_input(stdin)?;
    let (worktree, _) = resolve_agent(env)?;
    let rel = tools::write_summary(&worktree, &input.content).map_err(Error::WriteSummary)?;
    emit(stdout, "written", rel)
}

/// `mark_for_deletion`: stage the removal of a branch-relative path on the
/// compactor's worktree (§2.7). Production passes [`RealGit`] via
/// [`run_mark_for_deletion_with`]; the git op is injected for tests.
pub fn run_mark_for_deletion<R: Read, W: Write>(
    stdin: &mut R,
    stdout: &mut W,
    env: &dyn EnvLookup,
) -> Result<(), Error> {
    run_mark_for_deletion_with(stdin, stdout, env, &RealGit::new())
}

/// [`run_mark_for_deletion`] with the git runner injected.
pub fn run_mark_for_deletion_with<R: Read, W: Write>(
    stdin: &mut R,
    stdout: &mut W,
    env: &dyn EnvLookup,
    git: &dyn GitRunner,
) -> Result<(), Error> {
    let input: MarkInput = read_input(stdin)?;
    let (worktree, branch) = resolve_agent(env)?;
    tools::mark_for_deletion(&worktree, &branch, &input.path, git).map_err(Error::Mark)?;
    emit(stdout, "marked", input.path)
}

/// Parse the `tool_use.input` JSON from stdin.
fn read_input<R: Read, T: for<'de> Deserialize<'de>>(stdin: &mut R) -> Result<T, Error> {
    let mut buf = Vec::new();
    stdin.read_to_end(&mut buf).map_err(Error::StdinRead)?;
    serde_json::from_slice(&buf).map_err(Error::InvalidJson)
}

/// The calling agent's worktree **and its id**, from `LITANY_CONV_REPO`
/// (workspace) + `LITANY_CONV_BRANCH` (agent id), harness-derived (§3.3).
/// The id travels with the worktree because `mark_for_deletion`'s third
/// eligibility class is read against the compactor's own dispatch commit
/// (§2.7), which is named by the id and by nothing else.
fn resolve_agent(env: &dyn EnvLookup) -> Result<(std::path::PathBuf, String), Error> {
    let repo = env
        .get(ENV_CONV_REPO)
        .ok_or(Error::MissingEnv(ENV_CONV_REPO))?;
    let branch = env
        .get(ENV_CONV_BRANCH)
        .ok_or(Error::MissingEnv(ENV_CONV_BRANCH))?
        .into_string()
        .map_err(|_| Error::MissingEnv(ENV_CONV_BRANCH))?;
    let worktree = workspace::agent_worktree(Path::new(&repo), &branch);
    Ok((worktree, branch))
}

/// Serialize the `{status, path}` result to `stdout` (§3.3).
fn emit<W: Write>(stdout: &mut W, status: &str, path: String) -> Result<(), Error> {
    let bytes = serde_json::to_vec(&Output { status, path }).expect("Output serializes");
    stdout.write_all(&bytes).map_err(Error::Write)
}

#[cfg(test)]
mod tests;
