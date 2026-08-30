//! The agent's **hold mark** — `refs/litany/held/<agent-id>` (ARCH §3.3
//! *Tool control*).
//!
//! When the configured tool control answers **hold**, the seam parks the
//! invocation *before* it executes: no tool ran, no `tool_result`
//! committed, and the driver exits without a terminal. This mark is the
//! parked state's one non-derivable fact: "the named `tool_use` was held
//! before execution — nothing at or after it in its step has run." That
//! assertion is exactly what distinguishes a parked branch from the §6
//! *one non-replayable state* (a mid-tools crash, where a tool may have
//! run without committing), so `litany advance` re-enters the tool
//! window under the mark where it would otherwise decline loudly.
//!
//! It lives in the per-agent mark namespace ([`super::MARK_REF_ROOT`],
//! §2.2) beside `conflicted` / `budget-exhausted` / `abandoned` /
//! `notify` / `cwd`, so it is reaped with the agent by `litany delete`
//! (§9.2 enumerates the mark root) and crosses no fork, transfer or
//! merge. Like [`super::cwd`] it **carries a value**: the ref names a
//! blob holding one line of JSON ([`Held`]) — the held `tool_use` id,
//! the tool name, and the control's reason, `git cat-file`-readable by
//! an operator deciding whether to release. Release itself is not a
//! harness verb: the next drive of the agent re-consults the control
//! (§3.3), so whatever out-of-band fact lifts the hold is the control's
//! own contract.
//!
//! An unreadable or unparseable mark reads as absent ([`read`] →
//! `None`, the [`super::cwd`] discipline): the branch then falls back to
//! the loud §6 unpaired decline — conservative, never a forged result.

use super::{MARK_REF_ROOT, repo_git};
use crate::template::GitRunner;
use serde::{Deserialize, Serialize};
use std::io;
use std::path::Path;

/// Ref-namespace prefix for the hold mark (§3.3 *Tool control*).
pub const HOLD_REF_PREFIX: &str = "held/";

/// `refs/litany/held/<agent-id>` — the mark ref for one agent.
pub fn hold_ref(agent_id: &str) -> String {
    format!("{MARK_REF_ROOT}{HOLD_REF_PREFIX}{agent_id}")
}

/// The mark's value: which invocation was held, and why. One line of
/// JSON in the blob, so it survives the [`GitRunner::run_capture`]
/// trimmed-UTF-8 round trip (serde escapes any newline in `reason`).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Held {
    /// The `tool_use.id` of the parked invocation.
    pub tool_use_id: String,
    /// The tool the model named — for the operator's eyes; the id is
    /// what the resume matches on.
    pub tool: String,
    /// The control's stated reason for the hold.
    pub reason: String,
}

/// The agent's hold mark, or `None` when it is unset — the ordinary
/// state of every branch no control has parked. An unreadable or
/// unparseable mark reads the same way (module docs).
pub fn read(workspace: &Path, agent_id: &str, git: &dyn GitRunner) -> Option<Held> {
    let spec = hold_ref(agent_id);
    let out = git
        .run_capture(&repo_git(workspace), &["cat-file", "blob", &spec])
        .ok()?;
    serde_json::from_str(&out).ok()
}

/// Park `agent_id` on `held`: write the value blob and point the mark at
/// it — last write wins, so a re-adjudicated hold simply restates the
/// frontier. Same staging shape as [`super::cwd::write`]: the value is
/// staged beside the bare repo (never inside a worktree, so no `git add
/// -A` can see it) and hashed with `git hash-object`.
pub fn write(workspace: &Path, agent_id: &str, held: &Held, git: &dyn GitRunner) -> io::Result<()> {
    let repo = repo_git(workspace);
    let staged = repo.join(format!("hold-mark.{}.tmp", std::process::id()));
    let value = serde_json::to_string(held).expect("Held serializes");
    std::fs::write(&staged, value)?;
    let staged_str = staged.to_string_lossy().into_owned();
    let hashed = git.run_capture(&repo, &["hash-object", "-w", "--", &staged_str]);
    std::fs::remove_file(&staged)?;
    git.run(&repo, &["update-ref", &hold_ref(agent_id), &hashed?])
}

/// Lift the mark. Called where the mark is known present — the seam
/// re-adjudicating the held invocation to a pass or refuse, and the
/// stale-mark sweep (§3.3).
pub fn clear(workspace: &Path, agent_id: &str, git: &dyn GitRunner) -> io::Result<()> {
    git.run(
        &repo_git(workspace),
        &["update-ref", "-d", &hold_ref(agent_id)],
    )
}

#[cfg(test)]
mod tests;
