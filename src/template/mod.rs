//! Workspace creation and first config-commit authoring (ARCH §2.2).
//!
//! Embeds the [`template/`] directory at build time via `include_dir`,
//! so the `litany` binary is self-contained — no runtime template
//! lookup. [`scaffold`] creates the bare workspace repository at
//! `<dest>/repo.git` and authors the workspace's **first config
//! commit** — an orphan root on `config/default` (§2.2) — as the
//! harness-assisted act §2.2 describes: materialize a checkout, write
//! the control files from the embedded template (overlaid by any
//! `<config-root>/template/` override, §2.2) plus the
//! `descriptions/**` snapshot from the data-root pools (§3.3), commit,
//! and tear the checkout down. There is no `main` and no primary
//! worktree: agents fork off the config branch's head (§2.3), and the
//! fork *is* the freeze (§2.2).

pub mod authoring;
pub(crate) mod checkout;
pub mod descriptions;

use crate::harness_root::Roots;
use include_dir::{Dir, include_dir};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::process::Command;

/// The embedded config-commit template (ARCH §2.2). Holds the control
/// files a config commit carries — `manifest.yaml`, `workflow.yaml`,
/// `providers.yaml`, `version`, `souls/` — authored onto the orphan
/// `config/default` root by [`scaffold`].
pub static TEMPLATE: Dir<'_> = include_dir!("$CARGO_MANIFEST_DIR/template");

/// Config-root subdir overriding the embedded [`TEMPLATE`] (ARCH §2.2):
/// the seed set is the union of the embedded files with any same-named
/// file under `<config-root>/template/` winning, extra files included.
/// Absent dir = the embedded template alone. `litany prime` never seeds
/// it — absence is the default (policy lives in config, not code).
pub const TEMPLATE_OVERRIDE_DIR: &str = "template";

/// Errors [`scaffold`] can return.
#[derive(Debug, thiserror::Error)]
pub enum ScaffoldError {
    #[error("destination {0} already exists and is not empty")]
    DestNotEmpty(PathBuf),
    #[error("destination {0} already exists and is not a directory")]
    DestNotDir(PathBuf),
    #[error("I/O error: {0}")]
    Io(#[source] io::Error),
    #[error("git error: {0}")]
    Git(#[source] io::Error),
    #[error("descriptions-always: {0}")]
    Descriptions(#[source] descriptions::Error),
}

/// Abstraction over running `git` subcommands inside a target directory.
/// Implemented for [`RealGit`] by shelling out; tests supply their own
/// implementations to exercise the error paths in [`scaffold`].
pub trait GitRunner {
    /// Run `git <args>` with `-C dest`. Returns `Err` when the process
    /// cannot start or exits non-zero.
    fn run(&self, dest: &Path, args: &[&str]) -> io::Result<()>;

    /// Like [`GitRunner::run`], but captures stdout and returns it as a
    /// trimmed string. Used by commands that need the output (e.g.
    /// `git rev-parse HEAD` after a commit).
    fn run_capture(&self, dest: &Path, args: &[&str]) -> io::Result<String>;
}

/// `GitRunner` that invokes a `git` binary on disk.
///
/// The binary path is a field so tests can swap in a nonexistent path
/// to exercise the spawn-failure branch.
pub struct RealGit {
    bin: PathBuf,
}

impl RealGit {
    /// Use the `git` found on `PATH`.
    pub fn new() -> Self {
        Self {
            bin: PathBuf::from("git"),
        }
    }
}

impl Default for RealGit {
    fn default() -> Self {
        Self::new()
    }
}

impl GitRunner for RealGit {
    fn run(&self, dest: &Path, args: &[&str]) -> io::Result<()> {
        self.run_capture(dest, args).map(|_| ())
    }

    fn run_capture(&self, dest: &Path, args: &[&str]) -> io::Result<String> {
        // When invoked from a git-hook context, GIT_DIR / GIT_INDEX_FILE
        // / GIT_WORK_TREE / GIT_OBJECT_DIRECTORY are in the environment
        // and would cause the child `git` to operate on the outer repo
        // regardless of `-C`. Scrub them before spawning.
        let mut cmd = Command::new(&self.bin);
        for var in INHERITED_GIT_ENV {
            cmd.env_remove(var);
        }
        let out = cmd.arg("-C").arg(dest).args(args).output()?;
        if !out.status.success() {
            return Err(io::Error::other(format!(
                "git {args:?} exited with {}: {}",
                out.status,
                String::from_utf8_lossy(&out.stderr).trim()
            )));
        }
        Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
    }
}

const INHERITED_GIT_ENV: &[&str] = &[
    "GIT_DIR",
    "GIT_WORK_TREE",
    "GIT_INDEX_FILE",
    "GIT_OBJECT_DIRECTORY",
    "GIT_PREFIX",
    "GIT_COMMON_DIR",
    "GIT_ALTERNATE_OBJECT_DIRECTORIES",
];

/// Create a new workspace at `dest` per ARCH §2.2:
///
/// 1. Refuse if `dest` already exists and is non-empty.
/// 2. `git init --bare -b config/default <dest>/repo.git` — the
///    workspace repository. No `main` is ever created (§2.2).
/// 3. Author the first config commit (an orphan root, §2.2) through a
///    transient checkout: `git worktree add --orphan`, extract the
///    embedded [`TEMPLATE`] control files, overlay the
///    `<config-root>/template/` override ([`TEMPLATE_OVERRIDE_DIR`] —
///    same-named files win, extra files are included, an absent dir
///    changes nothing), snapshot the descriptions-always tree from the
///    data-root pools into `descriptions/{tools,skills}/` (ARCH §3.3 —
///    an empty or absent pool yields an empty descriptions tree),
///    `git add -A`, commit.
/// 4. Remove the authoring worktree. The workspace is left with exactly
///    one ref, `config/default`, whose head is the config commit every
///    fresh root agent forks off (§2.3) — the fork is the freeze.
pub fn scaffold<G: GitRunner>(dest: &Path, roots: &Roots, git: &G) -> Result<(), ScaffoldError> {
    check_dest(dest)?;
    let repo = crate::workspace::repo_git(dest);
    let config_ref = crate::workspace::config_ref(crate::workspace::DEFAULT_CONFIG_NAME);
    let config_ref = config_ref.as_str();
    fs::create_dir_all(&repo).map_err(ScaffoldError::Io)?;
    let init_args = ["init", "--bare", "-b", config_ref];
    git.run(&repo, &init_args).map_err(ScaffoldError::Git)?;

    let author = checkout::path(dest);
    let author_str = author.to_string_lossy().to_string();
    let mut add_args = vec!["worktree", "add", "--orphan", "-b", config_ref];
    add_args.push(author_str.as_str());
    // The guard tears the checkout down on every exit path below, so a
    // failed first commit leaves no half-authored checkout behind.
    let checkout = checkout::Checkout::add(git, &repo, &author, &add_args, None)
        .map_err(ScaffoldError::Git)?;

    // `git worktree add` creates the directory in production; the
    // explicit `create_dir_all` is for stub-git tests (and a harmless
    // no-op in production) — the same pattern as the subagent spawn.
    fs::create_dir_all(&author).map_err(ScaffoldError::Io)?;
    TEMPLATE.extract(&author).map_err(ScaffoldError::Io)?;
    overlay(&roots.config.join(TEMPLATE_OVERRIDE_DIR), &author).map_err(ScaffoldError::Io)?;
    descriptions::snapshot(&roots.data, &author).map_err(ScaffoldError::Descriptions)?;
    let msg = format!("config: init [{config_ref}]");
    // Always `true` here: the embedded template always writes files, so
    // the first commit's stage is never empty (the decline is
    // [`authoring`]'s case, where the origin's tree already exists).
    commit_checkout(git, &author, &msg).map_err(ScaffoldError::Git)?;
    checkout.landed().map_err(ScaffoldError::Git)?;
    Ok(())
}

/// Recursively copy `src` over `dst`, overwriting what exists — the
/// override half of the seed-set union (ARCH §2.2). A missing `src` is
/// the default (no override authored) and changes nothing; any other
/// read failure surfaces.
fn overlay(src: &Path, dst: &Path) -> io::Result<()> {
    let entries = match fs::read_dir(src) {
        Ok(entries) => entries,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(()),
        Err(e) => return Err(e),
    };
    for entry in entries {
        let entry = entry?;
        let to = dst.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            fs::create_dir_all(&to)?;
            overlay(&entry.path(), &to)?;
        } else {
            fs::copy(entry.path(), &to)?;
        }
    }
    Ok(())
}

/// Stage everything in an authoring checkout and commit it with `msg` —
/// the shared tail of config-commit authoring (ARCH §2.2), used by both
/// [`scaffold`] (the first config commit) and [`authoring::author`]
/// (every later one). Each git step's failure rides `io::Error`; the
/// caller maps it to its own error kind.
///
/// Returns whether a commit landed: an **empty stage** is the pass whose
/// edit changed nothing, which authors no commit and moves no branch
/// (§2.2). Asking the index (`status --porcelain`) rather than reading
/// git's refusal off a failed `commit` keeps that outcome a decision this
/// code makes, not a message it parses. Teardown is not here — it belongs
/// to the caller's [`checkout::Checkout`] guard, which runs on every exit
/// path including this one.
pub(crate) fn commit_checkout<G: GitRunner>(git: &G, author: &Path, msg: &str) -> io::Result<bool> {
    git.run(author, &["add", "-A"])?;
    if git
        .run_capture(author, &["status", "--porcelain"])?
        .is_empty()
    {
        return Ok(false);
    }
    git.run(author, &["commit", "-m", msg])?;
    Ok(true)
}

fn check_dest(dest: &Path) -> Result<(), ScaffoldError> {
    match fs::read_dir(dest) {
        Ok(mut entries) => {
            if entries.next().is_some() {
                Err(ScaffoldError::DestNotEmpty(dest.to_path_buf()))
            } else {
                Ok(())
            }
        }
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(()),
        // `read_dir` raises the same `NotADirectory` errno whether `dest`
        // itself is a non-directory (this ball's case) or `dest` doesn't
        // exist but has a non-directory ancestor (a `create_dir_all`
        // failure downstream, exercised by
        // `scaffold_surfaces_repo_dir_creation_failure`) — ask about
        // `dest` itself to tell the two apart.
        Err(e) if e.kind() == io::ErrorKind::NotADirectory && dest.exists() => {
            Err(ScaffoldError::DestNotDir(dest.to_path_buf()))
        }
        Err(e) => Err(ScaffoldError::Io(e)),
    }
}

#[cfg(test)]
mod tests;
#[cfg(test)]
mod tests_descriptions;
#[cfg(test)]
mod tests_dest;
#[cfg(test)]
mod tests_override;
#[cfg(test)]
mod tests_realgit;
#[cfg(test)]
mod tests_scaffold;
