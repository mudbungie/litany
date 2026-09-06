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
mod git;

pub use git::{GitRunner, RealGit};

use crate::harness_root::Roots;
use include_dir::{Dir, include_dir};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

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
    /// The seed set carried a facts file over its cap (ARCH §5.5) — an
    /// override's, since the embedded template ships none. Refused at
    /// the write like every later config commit's.
    #[error(transparent)]
    Facts(#[from] crate::facts::OverCap),
}

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
///    changes nothing — the seed home of a lineage's `facts.md`,
///    refused here when over its cap, [`crate::facts`]), snapshot the
///    descriptions-always tree from the
///    data-root pools into `descriptions/{tools,skills}/` (ARCH §3.3 —
///    an empty or absent pool yields an empty descriptions tree),
///    `git add -A`, commit.
/// 4. Remove the authoring worktree. The workspace is left with exactly
///    one ref, `config/default`, whose head is the config commit every
///    fresh root agent forks off (§2.3) — the lineage resolution
///    follows from then on (§2.2, bl-403b).
pub fn scaffold(dest: &Path, roots: &Roots, git: &dyn GitRunner) -> Result<(), ScaffoldError> {
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
    crate::facts::require_within_cap(&author)?;
    descriptions::snapshot(&roots.data, &author).map_err(ScaffoldError::Descriptions)?;
    let msg = format!("config: init [{config_ref}]");
    // Always `true` here: the embedded template always writes files, so
    // the first commit's stage is never empty (the decline is
    // [`authoring`]'s case, where the origin's tree already exists).
    commit_checkout(git, &author, &msg, false).map_err(ScaffoldError::Git)?;
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
pub(crate) fn commit_checkout(
    git: &dyn GitRunner,
    author: &Path,
    msg: &str,
    amend: bool,
) -> io::Result<bool> {
    git.run(author, &["add", "-A"])?;
    if git
        .run_capture(author, &["status", "--porcelain"])?
        .is_empty()
    {
        return Ok(false);
    }
    let mut args = vec!["commit", "-m", msg];
    // **Amend REPLACES the one commit on a branch, keeping its parent**
    // (bl-3c11). Only a proposal that already stands amends: a proposal
    // is one commit parented on the followed config commit, and
    // `litany proposal`'s freshness is `<branch>^` against the lineage
    // head — so a second act on the same branch must replace the commit,
    // never stack a second one, or the branch is instantly and
    // permanently stale. The unchanged-tree arm above still decides
    // first, so an act that adds nothing amends nothing.
    if amend {
        args.push("--amend");
    }
    git.run(author, &args)?;
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
