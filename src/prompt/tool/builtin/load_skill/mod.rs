//! `load_skill` built-in (ARCH §3.3 *Body-on-demand*).
//!
//! Stdin is the `tool_use.input` block as JSON: `{ "name": <string> }`.
//! Electing a skill *is* a tool call: on success the tool puts the skill
//! directory into the calling agent's worktree at `skills/<name>/`,
//! where the next context assembly composes it (§5.2 `skills/**`). The
//! copy lands with the tool result under the ordinary
//! commit-per-side-effect discipline (§2.3, §3.3), so the load is a
//! transcript entry plus a worktree commit — auditable and replayable
//! from the read-state commit, with no new channel.
//!
//! **Two homes, resolved in one order** (`docs/DESIGN_LEARNING_LOOP.md`
//! §3, ARCH §3.3). A **workspace skill** is a body committed in the
//! config lineage at `skills/<name>/`; a pool skill is a body in
//! `<data-root>/skills/<name>/`. This tool resolves the **followed
//! config commit** first ([`crate::workspace::current_config::current_config`] — the
//! same tip control resolves from at every step boundary) and the
//! install pool second. There is no shadowing arm and none is needed:
//! the config-authoring pass refuses a workspace skill whose name a
//! pool skill holds ([`crate::template::descriptions`]), so at most one
//! home ever answers a name. A workspace body is checked out of that
//! commit (`git checkout <commit> -- skills/<name>`, which writes *and*
//! stages, the idiom the descriptor cut uses); a pooled one is copied,
//! having no commit to come from.
//!
//! `archived` is not a loadable name: `skills/archived/<name>/` is the
//! archive container (§5), whose bodies compose nowhere, and
//! `archived/<name>` is already refused as not a single path component.
//! The bare container name is refused beside it so no election can
//! smuggle the whole archive in as one directory.
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
use std::path::PathBuf;
use thiserror::Error;

mod homes;

use super::super::{ENV_CONV_BRANCH, ENV_CONV_REPO};
use super::dispatch::EnvLookup;
use crate::harness_root;
use crate::template::{GitRunner, RealGit, descriptions};
use crate::workspace;
use homes::{copy_dir, followed_commit, skills_pool, unknown};

/// Worktree subdirectory holding loaded skill bodies (§2.2, §5.2). Also
/// the data-root pool subdirectory (`<data-root>/skills/`, §3.3) and the
/// config lineage's workspace-skill home
/// (`docs/DESIGN_LEARNING_LOOP.md` §3).
const SKILLS_DIR: &str = crate::workspace::SKILLS_DIR;
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
    #[error(
        "skill name {0:?} is the archive container, not a skill \
         (docs/DESIGN_LEARNING_LOOP.md §5) — an archived body composes nowhere"
    )]
    Archived(String),
    #[error("resolve the followed config commit: {0}")]
    Lineage(#[source] io::Error),
    #[error("check out workspace skill {name:?} from {commit}: {source}")]
    Checkout {
        name: String,
        commit: String,
        #[source]
        source: io::Error,
    },
    #[error("unknown skill {name:?}; workspace skills: {workspace}; install pool: {pool}")]
    Unknown {
        name: String,
        workspace: String,
        pool: String,
    },
    #[error("copy skill {name:?} into worktree: {source}")]
    Copy {
        name: String,
        #[source]
        source: io::Error,
    },
    #[error("write to stdout: {0}")]
    Write(#[source] io::Error),
}

/// Parse stdin, resolve the two skill homes from the harness env, put
/// the skill body in the worktree (or report it already loaded), and
/// write the JSON status to `stdout`. Production wires the live process
/// stdio, [`super::dispatch::ProcessEnv`] and the real `git`.
pub fn run<R: Read, W: Write>(
    stdin: &mut R,
    stdout: &mut W,
    env: &dyn EnvLookup,
) -> Result<(), Error> {
    run_with(stdin, stdout, env, &RealGit::new())
}

/// [`run`] with the git runner injected — pure over [`Read`]/[`Write`],
/// the [`EnvLookup`] and [`GitRunner`], so unit tests drive it with
/// `Cursor`/`Vec` and a stub env against a real fixture workspace.
pub fn run_with<R: Read, W: Write>(
    stdin: &mut R,
    stdout: &mut W,
    env: &dyn EnvLookup,
    git: &dyn GitRunner,
) -> Result<(), Error> {
    let mut buf = Vec::new();
    stdin.read_to_end(&mut buf).map_err(Error::StdinRead)?;
    let input: Input = serde_json::from_slice(&buf).map_err(Error::InvalidJson)?;
    let name = input.name;
    // A skill name must address exactly one directory in one home — the
    // shared single-component rule (`crate::name`), the same guard the
    // command surface runs over an agent id.
    if !crate::name::is_component(&name) {
        return Err(Error::BadName(name));
    }
    if name == descriptions::ARCHIVED_SUBDIR {
        return Err(Error::Archived(name));
    }

    let repo = require_env(env, ENV_CONV_REPO)?;
    let branch = require_env(env, ENV_CONV_BRANCH)?
        .into_string()
        .map_err(|_| Error::MissingEnv(ENV_CONV_BRANCH))?;

    // Chain kept on one line: tarpaulin's llvm engine mis-attributes a
    // multi-line method chain's tail as uncovered (a known quirk).
    let workspace_dir = PathBuf::from(&repo);
    let worktree = workspace::agent_worktree(&workspace_dir, &branch);
    let dest = worktree.join(SKILLS_DIR).join(&name);
    let rel = format!("{SKILLS_DIR}/{name}");

    // Idempotent: an already-loaded copy wins (snapshot discipline, §3.3).
    if dest.is_dir() {
        return emit(stdout, STATUS_ALREADY_LOADED, rel);
    }

    // The followed config commit answers first (§3): a workspace skill
    // is the lineage's own, and the pool is the install's fallback.
    let commit = followed_commit(&workspace_dir, &branch, git)?;
    let committed = format!("{commit}:{rel}");
    if git.run(&worktree, &["cat-file", "-e", &committed]).is_ok() {
        // `git checkout <commit> -- <path>` writes *and* stages, so the
        // tool commit carries the body with no second `add`.
        git.run(&worktree, &["checkout", &commit, "--", &rel])
            .map_err(|source| Error::Checkout {
                name,
                commit,
                source,
            })?;
        return emit(stdout, STATUS_LOADED, rel);
    }

    let src = skills_pool(env)?.join(&name);
    if !src.is_dir() {
        // Struct literal kept on one line: the same llvm-engine
        // attribution quirk as the chain above marks a multi-line
        // `return Err(...)` literal's head line uncovered.
        return Err(unknown(name, &worktree, &commit, &skills_pool(env)?, git));
    }
    copy_dir(&src, &dest).map_err(|source| Error::Copy {
        name: name.clone(),
        source,
    })?;
    emit(stdout, STATUS_LOADED, rel)
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
#[cfg(test)]
mod tests_workspace;
