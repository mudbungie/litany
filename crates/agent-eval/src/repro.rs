//! Probing the reproducibility inputs (bl-36fa, ARCH §9.3): suite
//! revision, starting fixture identity, driver version. Each probe
//! *observes*; none assumes. A probe that fails yields `None`, rendered
//! as unknown/unreported — never a fabricated value (the same
//! missing-is-not-zero rule the usage counters follow).

use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

/// Git revision of the suite directory: `HEAD`, suffixed `+dirty` when
/// the suite subtree has uncommitted changes. `None` when the directory
/// is not inside a git checkout (or git itself is unavailable).
pub fn suite_revision(dir: &Path) -> Option<String> {
    let head = git(dir, &["rev-parse", "HEAD"])?;
    let dirty = git(dir, &["status", "--porcelain", "--", "."])?;
    Some(if dirty.is_empty() {
        head
    } else {
        format!("{head}+dirty")
    })
}

/// One git query in `dir`; `None` unless it ran and exited 0. The
/// inherited `GIT_*` redirection is scrubbed: the eval may itself run
/// under a git hook, and a leaked `GIT_DIR` would silently report some
/// *other* repository's revision as the suite's (the same trap the
/// shipped driver scrubs before exec'ing `litany`).
fn git(dir: &Path, args: &[&str]) -> Option<String> {
    let mut cmd = Command::new("git");
    cmd.args(args).current_dir(dir).stdin(Stdio::null());
    for var in [
        "GIT_DIR",
        "GIT_WORK_TREE",
        "GIT_INDEX_FILE",
        "GIT_OBJECT_DIRECTORY",
        "GIT_PREFIX",
        "GIT_COMMON_DIR",
        "GIT_ALTERNATE_OBJECT_DIRECTORIES",
    ] {
        cmd.env_remove(var);
    }
    let out = cmd.output().ok()?;
    out.status
        .success()
        .then(|| String::from_utf8_lossy(&out.stdout).trim().to_string())
}

/// Starting fixture identity: sha256 over the suite's task files (the
/// same sorted `*.yaml` set the loader reads — their `setup`/`check`
/// scripts define every run's starting state, so the digest identifies
/// the fixture even off a dirty or non-git tree). `None` when any file
/// is unreadable.
pub fn fixture_digest(dir: &Path) -> Option<String> {
    let mut files: Vec<PathBuf> = std::fs::read_dir(dir)
        .ok()?
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "yaml"))
        .collect();
    files.sort();
    let mut hasher = Sha256::new();
    for path in files {
        hasher.update(path.file_name()?.to_string_lossy().as_bytes());
        hasher.update([0]);
        hasher.update(std::fs::read(&path).ok()?);
        hasher.update([0]);
    }
    Some(
        hasher
            .finalize()
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect(),
    )
}

/// The driver's self-reported identity: `<program> --version`, first
/// stdout line. The README driver contract asks a driver to answer
/// `--version` as argv\[1\] with one identifying line; a driver that
/// fails, hangs up, or prints nothing is recorded as `None`
/// (unreported), never guessed at.
pub fn driver_version(program: &str) -> Option<String> {
    let out = Command::new(program)
        .arg("--version")
        .stdin(Stdio::null())
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&out.stdout);
    let line = text.lines().next()?.trim().to_string();
    (!line.is_empty()).then_some(line)
}
