//! The agent's **working-directory mark** — `refs/litany/cwd/<agent-id>`
//! (ARCH §3.3 *Working directory*).
//!
//! An agent's working directory is one mutable per-agent fact: its
//! worktree by default, and thereafter whatever its own `cd` tool call
//! last set. This module is that fact's one home. It lives in the
//! per-agent **mark** namespace ([`super::MARK_REF_ROOT`], §2.2) beside
//! `conflicted` / `budget-exhausted` / `abandoned` / `notify`, so it is
//! reaped with the agent by `litany delete` (§9.2 enumerates the mark
//! root, never a list of kinds), it crosses no fork and no transfer
//! (marks are keyed by agent id and nothing merges them), and it is not
//! context (§5.1 — the agent learns its cwd from the tool result, not
//! from its tree).
//!
//! **This mark carries a value where the others are bare assertions:**
//! the ref names a *blob* whose bytes are the absolute path. A ref may
//! name any object, so no second mechanism is needed to hold the one
//! extra fact — and `git gc` keeps the blob alive for exactly as long as
//! the mark does.
//!
//! The value round-trips through [`GitRunner::run_capture`], which
//! returns trimmed UTF-8, so [`write`] declines a directory whose path is
//! not preserved by that round trip rather than storing one that would
//! read back wrong (PRINCIPLES "Decline illegal operations").
//!
//! **The mark has two writers, and one validation.** The agent's own `cd`
//! built-in writes it mid-run; `litany prompt --cwd` / `litany dispatch
//! --cwd` seed it at creation, before the agent's first step (ARCH §3.3,
//! §2.5). Both reach a directory through [`resolve`], so a path is
//! refused in one voice wherever it was named — a second set of rules for
//! the seed would be a second answer to "what is a working directory".

use super::{MARK_REF_ROOT, repo_git};
use crate::template::GitRunner;
use std::io;
use std::os::unix::ffi::OsStrExt;
use std::path::{Path, PathBuf};
use thiserror::Error;

/// Ref-namespace prefix for the working-directory mark (§3.3).
pub const CWD_REF_PREFIX: &str = "cwd/";

/// `refs/litany/cwd/<agent-id>` — the mark ref for one agent.
pub fn cwd_ref(agent_id: &str) -> String {
    format!("{MARK_REF_ROOT}{CWD_REF_PREFIX}{agent_id}")
}

/// The agent's stored working directory, or `None` when the mark is
/// unset — which is the ordinary state of an agent that never called
/// `cd`, not an error. An unreadable mark (no repo, a ref pointing at a
/// non-blob, a git that would not run) reads the same way: the caller's
/// default applies, and no tool call is lost to a mark.
pub fn read(workspace: &Path, agent_id: &str, git: &dyn GitRunner) -> Option<PathBuf> {
    let spec = cwd_ref(agent_id);
    let out = git
        .run_capture(&repo_git(workspace), &["cat-file", "blob", &spec])
        .ok()?;
    (!out.is_empty()).then(|| PathBuf::from(out))
}

/// The agent's **effective** working directory (ARCH §3.3 *Resolution
/// at spawn*): the mark when it names a live directory, else the
/// agent's worktree. One home for the rule, because two readers ask it
/// — the executor, resolving where a tool subprocess runs
/// ([`crate::prompt::tool::spawn`]), and the tool window, deciding
/// which context files sit on that path
/// ([`crate::prompt::dispatch`]). A mark whose directory has since
/// disappeared answers the worktree rather than nothing: `cd` is itself
/// a tool call, so a hard decline would strand the agent somewhere it
/// could never leave.
pub fn effective(workspace: &Path, agent_id: &str, git: &dyn GitRunner) -> PathBuf {
    let worktree = super::agent_worktree(workspace, agent_id);
    read(workspace, agent_id, git)
        .filter(|p| p.is_dir())
        .unwrap_or(worktree)
}

/// Set the agent's working-directory mark to `dir` (an absolute path the
/// caller has already resolved and proven to be a directory). Writes the
/// path as a blob and points the mark at it — last write wins, exactly
/// as a `cd` should.
pub fn write(workspace: &Path, agent_id: &str, dir: &Path, git: &dyn GitRunner) -> io::Result<()> {
    storable(dir).map_err(io::Error::other)?;
    let repo = repo_git(workspace);
    // `git hash-object` reads a file; the trait's two methods carry no
    // stdin, so the value is staged beside the repo under a pid-unique
    // name and removed once hashed. It is never inside a worktree, so
    // no `git add -A` can see it (§3.3 commit-per-side-effect).
    let staged = repo.join(format!("cwd-mark.{}.tmp", std::process::id()));
    std::fs::write(&staged, dir.as_os_str().as_bytes())?;
    let staged_str = staged.to_string_lossy().into_owned();
    let hashed = git.run_capture(&repo, &["hash-object", "-w", "--", &staged_str]);
    std::fs::remove_file(&staged)?;
    git.run(&repo, &["update-ref", &cwd_ref(agent_id), &hashed?])
}

/// Every way a caller-named directory can fail to be a working
/// directory. One taxonomy for both writers (module docs): the `cd`
/// built-in re-emits it as its `is_error` `tool_result`, `--cwd` as the
/// verb's own refusal before the fork.
#[derive(Debug, Error)]
pub enum ResolveError {
    #[error("no such directory {path:?}: {source}")]
    NoSuchDir {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("{path:?} is not a directory — a working directory is one (ARCH §3.3)")]
    NotADir { path: PathBuf },
    #[error(
        "cannot store {path:?} as a working directory: the mark holds trimmed UTF-8 text, so \
         a path that is not UTF-8 or that leads or trails with whitespace would read back as \
         a different directory (ARCH §3.3)"
    )]
    NotStorable { path: PathBuf },
}

/// The absolute directory `path` names, ready to become a mark:
/// canonicalized, proven to be a directory, and proven to survive the
/// mark's round trip ([`storable`]).
///
/// **Relative paths need no resolution of ours.** `canonicalize` resolves
/// against this process's own working directory — which the executor set
/// to the agent's, when the caller is the `cd` built-in (§3.3), and which
/// is the operator's shell, when it is `--cwd`. Either way it is the
/// kernel's answer, `..` and symlinks included, not a re-derivation of
/// one. A path that names nothing and a path that names a non-directory
/// are declined separately: they are different mistakes.
pub fn resolve(path: &Path) -> Result<PathBuf, ResolveError> {
    let abs = std::fs::canonicalize(path).map_err(|source| ResolveError::NoSuchDir {
        path: path.to_owned(),
        source,
    })?;
    if !abs.is_dir() {
        return Err(ResolveError::NotADir {
            path: path.to_owned(),
        });
    }
    storable(&abs)?;
    Ok(abs)
}

/// Can `dir` survive the mark's storage round trip — written as bytes,
/// read back as trimmed UTF-8? A non-UTF-8 path or one with leading or
/// trailing whitespace cannot, and is declined here rather than stored
/// to read back as some other directory.
fn storable(dir: &Path) -> Result<(), ResolveError> {
    let text = dir.to_str().filter(|s| !s.is_empty() && s.trim() == *s);
    match text {
        Some(_) => Ok(()),
        None => Err(ResolveError::NotStorable {
            path: dir.to_owned(),
        }),
    }
}

#[cfg(test)]
mod tests;
