//! The `git` seam every authoring act runs through (ARCH §2.2): the
//! injectable [`GitRunner`], the [`RealGit`] that shells out, and the
//! ambient-environment scrub it performs on every fork.
//!
//! Split out of [`super`] at the per-file cap (bl-3c11). It is a real
//! seam and not a shave: the rest of that module is *what* the template
//! authors — the embedded skeleton, the scaffold, the commit — while
//! this is *how* any of it reaches git, and it is the half every other
//! module of the crate names. Both types stay re-exported at
//! `template::*`, which is the path every consumer already spells.

use std::io;
use std::path::{Path, PathBuf};
use std::process::Command;

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

    /// [`GitRunner::run_capture`] without the lossy round trip: raw
    /// stdout bytes, untrimmed. The one caller that needs it is
    /// `search_history` (ARCH §3.3), which hands stored transcript
    /// entries back to the model *verbatim* — a trimmed, lossily
    /// re-encoded blob is not the entry that was committed, and the
    /// address it advertises would then recover something else.
    /// Defaulted to the trimmed capture so the many test doubles owe no
    /// second answer; [`RealGit`] overrides it with the real bytes and
    /// derives `run_capture` from it, so the scrub below has one home.
    fn run_capture_bytes(&self, dest: &Path, args: &[&str]) -> io::Result<Vec<u8>> {
        self.run_capture(dest, args).map(String::into_bytes)
    }
}

/// `GitRunner` that invokes a `git` binary on disk.
///
/// The binary path is a field so tests can swap in a nonexistent path
/// to exercise the spawn-failure branch.
pub struct RealGit {
    /// `pub(super)` since the split (bl-3c11): the spawn-failure beat
    /// lives beside the rest of the template's tests, one module up.
    pub(super) bin: PathBuf,
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
        self.run_capture_bytes(dest, args).map(|_| ())
    }

    fn run_capture(&self, dest: &Path, args: &[&str]) -> io::Result<String> {
        self.run_capture_bytes(dest, args)
            .map(|out| String::from_utf8_lossy(&out).trim().to_string())
    }

    fn run_capture_bytes(&self, dest: &Path, args: &[&str]) -> io::Result<Vec<u8>> {
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
        Ok(out.stdout)
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
