//! The transient authoring checkout and its teardown (ARCH §2.2).
//!
//! Both config-commit authoring acts — [`super::scaffold`] for a
//! workspace's first commit, [`super::authoring::author`] for every later
//! one — materialize one checkout at `<workspace>/.config-author`, edit
//! it, commit, and tear it down. This module owns the teardown so it
//! **cannot be skipped**: [`Checkout`] is a drop guard, so the decline, a
//! git failure, an editor failure and an early `?` all tear down by the
//! same path the success case does. Anything a *killed* pass leaves
//! behind is cleared by [`heal`] at the next pass's start — ARCH §2.11's
//! crash philosophy ("everything is on disk … the next touch heals it")
//! applied to this checkout, so a hard kill never wedges the verb behind
//! a hand-run `git worktree remove`.

use super::GitRunner;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

/// Transient authoring checkout under the workspace root; removed once
/// the pass ends, however it ends. One at a time — authoring a config
/// commit is a deliberate single-user act (ARCH §1.1).
const AUTHOR_DIR: &str = ".config-author";

/// The authoring checkout's path inside `workspace`.
pub(crate) fn path(workspace: &Path) -> PathBuf {
    workspace.join(AUTHOR_DIR)
}

/// Clear the checkout path so a pass can materialize on it, whatever an
/// interrupted predecessor left (ARCH §2.11): a stale registration whose
/// directory is gone is git's own `prune`, and a surviving directory is
/// debris this removes. Unconditional — on the ordinary path there is
/// nothing there and every step is a no-op, so there is no leftover
/// special case to get wrong, and no state a hard kill can leave that
/// makes the *next* `litany config` fail. The transient checkout holds no
/// authored history by construction (its content is the origin's tree
/// plus the pools plus an uncommitted edit), so removing it loses only
/// the killed pass's unsaved edit. A path that survives all three steps
/// surfaces as an error rather than a silent wedge.
pub(crate) fn heal(git: &dyn GitRunner, repo: &Path, path: &Path) -> io::Result<()> {
    let lossy = path.to_string_lossy();
    let _ = git.run(repo, &["worktree", "prune"]);
    let _ = git.run(repo, &remove_args(&lossy));
    match fs::remove_dir_all(path) {
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(()),
        other => other,
    }
}

/// A materialized authoring checkout, torn down when it drops.
///
/// Construction is the `git worktree add`; teardown is [`Drop`], so every
/// exit path out of an authoring pass — including the ones that never
/// reach a commit — removes the checkout. `created` names the ref the add
/// itself created (`-b`, i.e. a fork or an orphan lineage); unless
/// [`Checkout::landed`] is called it is deleted along with the checkout,
/// so a failed or declined pass leaves no dangling `config/<name>`.
pub(crate) struct Checkout<'a> {
    git: &'a dyn GitRunner,
    repo: &'a Path,
    /// The checkout path as git's argv wants it — the form every
    /// teardown step spends it in.
    path: String,
    created: Option<String>,
    armed: bool,
}

impl<'a> Checkout<'a> {
    /// Materialize the checkout by running `add_args` (the origin's
    /// `worktree add` form) in `repo`, and arm the teardown. A failing
    /// add creates no guard: there is nothing to tear down.
    // `#[rustfmt::skip]` keeps the `Self` literal on one line: exploded
    // across field lines, tarpaulin's llvm engine mis-attributes those
    // lines as uncovered (the known multi-line quirk `cmd::tests` and
    // `tool::builtin` document); every field here runs on every call.
    #[rustfmt::skip]
    pub(crate) fn add(
        git: &'a dyn GitRunner,
        repo: &'a Path,
        path: &Path,
        add_args: &[&str],
        created: Option<String>,
    ) -> io::Result<Self> {
        git.run(repo, add_args)?;
        let path = path.to_string_lossy().into_owned();
        Ok(Self { git, repo, path, created, armed: true })
    }

    /// The commit landed: tear the checkout down, keeping the ref the
    /// pass created. Teardown failure surfaces here — on the success path
    /// it is the pass's only news; on every failure path [`Drop`] runs it
    /// best-effort instead, because the error already in flight is the
    /// one to report and [`heal`] clears whatever was left.
    pub(crate) fn landed(mut self) -> io::Result<()> {
        self.armed = false;
        self.git.run(self.repo, &remove_args(&self.path))
    }
}

impl Drop for Checkout<'_> {
    fn drop(&mut self) {
        if self.armed {
            let _ = self.git.run(self.repo, &remove_args(&self.path));
            if let Some(created) = &self.created {
                let _ = self.git.run(self.repo, &["branch", "-D", created]);
            }
        }
    }
}

/// `worktree remove` for `path`. `--force` because the failure paths tear
/// down a dirty checkout (an edit that was never committed), and the
/// success path's checkout is clean, where it changes nothing — one form
/// for both.
fn remove_args(path: &str) -> [&str; 4] {
    ["worktree", "remove", "--force", path]
}
