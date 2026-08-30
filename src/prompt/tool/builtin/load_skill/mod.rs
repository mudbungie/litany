//! `load_skill` built-in (ARCH §3.3 *Body-on-demand*).
//!
//! Stdin is the `tool_use.input` block as JSON: `{ "name": <string> }`.
//! Electing a skill *is* a tool call: on success the tool copies the
//! skill directory `<data-root>/skills/<name>/` into the calling agent's
//! worktree at `skills/<name>/`, where the next context assembly composes
//! it (§5.2 `skills/**`). The copy lands with the tool result under the
//! ordinary commit-per-side-effect discipline (§2.3, §3.3), so the load
//! is a transcript entry plus a worktree commit — auditable and
//! replayable from the read-state commit, with no new channel.
//!
//! **Copy, not symlink** (§3.3): the loaded body is self-contained and
//! survives the data-root pool changing or disappearing. **Snapshot, not
//! mirror:** an already-present `skills/<name>/` is reported
//! `already_loaded` and left untouched even if the pool has since changed
//! — the loaded copy is the snapshot the branch is pinned to (§3.3,
//! PRINCIPLES single source of truth). Picking up a newer pool version is
//! `rm skills/<name>` then reload, an ordinary curation act (§5.4).
//!
//! The calling agent's workspace + branch arrive via `LITANY_CONV_REPO` /
//! `LITANY_CONV_BRANCH` (§3.3, harness-derived); the data root resolves
//! from the same `LITANY_HOME` / XDG env the harness reads
//! ([`crate::harness_root`]). An unknown `name` is declined —
//! `is_error` naming the available pool — never fuzzy-matched (PRINCIPLES
//! decline illegal operations).

use serde::{Deserialize, Serialize};
use std::ffi::OsString;
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use thiserror::Error;

use super::super::{ENV_CONV_BRANCH, ENV_CONV_REPO};
use super::dispatch::EnvLookup;
use crate::harness_root;
use crate::workspace;

/// Worktree subdirectory holding loaded skill bodies (§2.2, §5.2). Also
/// the data-root pool subdirectory (`<data-root>/skills/`, §3.3).
const SKILLS_DIR: &str = "skills";
/// Env keys the data-root resolution reads (mirrors [`harness_root`]).
const ENV_LITANY_HOME: &str = "LITANY_HOME";
const ENV_XDG_DATA: &str = "XDG_DATA_HOME";
const ENV_HOME: &str = "HOME";

/// Wire shape of the input. `deny_unknown_fields` so a malformed
/// `tool_use.input` surfaces as [`Error::InvalidJson`] rather than a
/// silent drop.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Input {
    name: String,
}

/// Wire shape of the output — the `tool_result.content` payload. `status`
/// distinguishes a fresh copy from an idempotent no-op; `path` is the
/// worktree-relative location the body now lives at (`skills/<name>`).
#[derive(Debug, Serialize, PartialEq, Eq)]
struct Output<'a> {
    status: &'a str,
    path: String,
}
const STATUS_LOADED: &str = "loaded";
const STATUS_ALREADY_LOADED: &str = "already_loaded";

/// Every way [`run`] can fail. Each variant prints its own stderr
/// message; per §3.3 stderr concatenates after stdout into
/// `tool_result.content` when exit is non-zero, so the model sees the
/// decline verbatim.
#[derive(Debug, Error)]
pub enum Error {
    #[error("invalid input JSON: {0}")]
    InvalidJson(#[source] serde_json::Error),
    #[error("read input from stdin: {0}")]
    StdinRead(#[source] io::Error),
    #[error("missing env var {0:?} (set by the harness per ARCH §3.3)")]
    MissingEnv(&'static str),
    #[error("resolve data root: {0}")]
    Root(#[source] harness_root::Error),
    #[error("skill name {0:?} is not a single path component (ARCH §3.3)")]
    BadName(String),
    #[error("unknown skill {name:?}; available: {available}")]
    Unknown { name: String, available: String },
    #[error("copy skill {name:?} into worktree: {source}")]
    Copy {
        name: String,
        #[source]
        source: io::Error,
    },
    #[error("write to stdout: {0}")]
    Write(#[source] io::Error),
}

/// Parse stdin, resolve the pool + worktree from the harness env, copy
/// the skill body in (or report it already loaded), and write the JSON
/// status to `stdout`. Pure over [`Read`]/[`Write`] + the injected
/// [`EnvLookup`] so unit tests drive it with `Cursor`/`Vec` and a stub
/// env; the `litany tool load_skill` shim wires the live process stdio
/// plus [`super::dispatch::ProcessEnv`].
pub fn run<R: Read, W: Write>(
    stdin: &mut R,
    stdout: &mut W,
    env: &dyn EnvLookup,
) -> Result<(), Error> {
    let mut buf = Vec::new();
    stdin.read_to_end(&mut buf).map_err(Error::StdinRead)?;
    let input: Input = serde_json::from_slice(&buf).map_err(Error::InvalidJson)?;
    let name = input.name;
    // A skill name must address exactly one directory in the pool — the
    // shared single-component rule (`crate::name`), the same guard the
    // command surface runs over an agent id.
    if !crate::name::is_component(&name) {
        return Err(Error::BadName(name));
    }

    let repo = require_env(env, ENV_CONV_REPO)?;
    let branch = require_env(env, ENV_CONV_BRANCH)?
        .into_string()
        .map_err(|_| Error::MissingEnv(ENV_CONV_BRANCH))?;

    // Chain kept on one line: tarpaulin's llvm engine mis-attributes a
    // multi-line method chain's tail as uncovered (a known quirk).
    let worktree = workspace::agent_worktree(Path::new(&repo), &branch);
    let dest = worktree.join(SKILLS_DIR).join(&name);
    let rel = format!("{SKILLS_DIR}/{name}");

    // Idempotent: an already-loaded copy wins (snapshot discipline, §3.3).
    if dest.is_dir() {
        return emit(stdout, STATUS_ALREADY_LOADED, rel);
    }

    let src = skills_pool(env)?.join(&name);
    if !src.is_dir() {
        // Struct literal kept on one line: the same llvm-engine
        // attribution quirk as the chain above marks a multi-line
        // `return Err(...)` literal's head line uncovered.
        let available = available(&skills_pool(env)?);
        return Err(Error::Unknown { name, available });
    }
    copy_dir(&src, &dest).map_err(|source| Error::Copy {
        name: name.clone(),
        source,
    })?;
    emit(stdout, STATUS_LOADED, rel)
}

/// The data-root skills pool `<data-root>/skills/` (§3.3). Resolved from
/// the same env [`harness_root`] reads, injected via [`EnvLookup`] so the
/// tool stays pure over its environment for tests.
fn skills_pool(env: &dyn EnvLookup) -> Result<PathBuf, Error> {
    let override_v = env.get(ENV_LITANY_HOME);
    let xdg_data = env.get(ENV_XDG_DATA);
    let home = env.get(ENV_HOME);
    let roots = harness_root::resolve_from(
        override_v.as_deref(),
        None,
        xdg_data.as_deref(),
        home.as_deref().map(Path::new),
    )
    .map_err(Error::Root)?;
    Ok(roots.data.join(SKILLS_DIR))
}

/// Comma-joined, sorted list of the pool's skill directory names for the
/// decline message, rendered by the shared [`crate::name::pool`] idiom. A
/// missing or unreadable pool reads as `(none)` — the decline still names
/// *that* there is nothing to load.
fn available(pool: &Path) -> String {
    let mut names: Vec<String> = match std::fs::read_dir(pool) {
        Ok(rd) => rd
            .flatten()
            .filter(|e| e.path().is_dir())
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .collect(),
        Err(_) => Vec::new(),
    };
    names.sort();
    crate::name::pool(&names)
}

/// Recursively copy `src` into `dest`, creating `dest` and any parents.
/// Plain byte copies of files under a mirrored directory tree — the same
/// portability discipline `make install` uses for the pool (§3.3).
fn copy_dir(src: &Path, dest: &Path) -> io::Result<()> {
    std::fs::create_dir_all(dest)?;
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let from = entry.path();
        let to = dest.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            copy_dir(&from, &to)?;
        } else {
            std::fs::copy(&from, &to)?;
        }
    }
    Ok(())
}

/// Serialize the [`Output`] payload to `stdout`.
fn emit<W: Write>(stdout: &mut W, status: &str, path: String) -> Result<(), Error> {
    let payload = Output { status, path };
    let bytes = serde_json::to_vec(&payload).expect("Output is always serializable");
    stdout.write_all(&bytes).map_err(Error::Write)
}

/// Read a required harness env var or fail with [`Error::MissingEnv`].
fn require_env(env: &dyn EnvLookup, key: &'static str) -> Result<OsString, Error> {
    env.get(key).ok_or(Error::MissingEnv(key))
}

#[cfg(test)]
mod tests;
