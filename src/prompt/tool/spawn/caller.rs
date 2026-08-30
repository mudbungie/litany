//! The calling agent of one tool call — derived, never handed in
//! (ARCH §3.3).
//!
//! One derivation off the executor's `step_dir` feeds both halves of the
//! §3.3 subprocess contract, the environment and the working directory,
//! so the cwd a tool runs in and the worktree its `LITANY_CONV_*` vars
//! name cannot disagree.

use crate::prompt::step::STEPS_DIR;
use crate::template::GitRunner;
use crate::workspace;
use std::ffi::OsString;
use std::path::{Path, PathBuf};

/// The calling agent, derived from the executor's `step_dir` —
/// `<workspace>/steps/<agent-id>/<NNN>` (ARCH §2.2). One derivation
/// feeds both halves of the §3.3 subprocess contract, the environment
/// and the working directory, so the cwd a tool runs in and the
/// worktree its `LITANY_CONV_*` vars name cannot disagree. No caller
/// hands these in: the executor is the single source of truth for what
/// a tool call is on behalf of.
pub(super) struct Caller {
    /// `<workspace>` — `LITANY_CONV_REPO`, and the same fact a host
    /// router reads as `RoutedCall::workspace` (§3.3).
    pub(super) workspace: PathBuf,
    /// The agent id (== full hyphenated descent, §2.3) —
    /// `LITANY_CONV_BRANCH`, and `RoutedCall::agent`.
    pub(super) agent_id: String,
    /// The cwd of every subprocess this call spawns (§3.3 *Working
    /// directory*): `<workspace>/agents/<agent-id>` by default, or
    /// whatever the agent's own `cd` last set (`workspace::cwd`).
    pub(super) cwd: PathBuf,
}

impl Caller {
    /// Read the workspace root and agent id back out of `step_dir`,
    /// materialize the worktree path they name, and resolve the working
    /// directory the tool will run in. `None` when `step_dir` is not the
    /// §2.2 shape or the worktree it names is not a live directory — the
    /// executor declines the call rather than running the tool in an
    /// inherited cwd.
    ///
    /// The worktree is the **default** cwd, not the only one: the agent's
    /// own `cd` calls store a working-directory mark
    /// ([`workspace::cwd`], §3.3), and a mark naming a live directory
    /// wins. A mark whose directory has since disappeared falls back to
    /// the worktree rather than declining the call — `cd` is itself a
    /// tool call, so a hard decline would leave the agent no way back.
    pub(super) fn resolve(step_dir: &Path, git: &dyn GitRunner) -> Option<Self> {
        // step_dir = <workspace>/steps/<agent-id>/<NNN>; ascend one for
        // the agent-id segment, three to reach the workspace root.
        let agent_dir = step_dir.parent()?;
        let agent_id = agent_dir.file_name()?.to_str()?.to_string();
        let workspace = agent_dir
            .parent()
            .filter(|p| p.ends_with(STEPS_DIR))?
            .parent()?;
        let worktree = workspace::agent_worktree(workspace, &agent_id);
        if !worktree.is_dir() {
            return None;
        }
        let moved = workspace::cwd::read(workspace, &agent_id, git);
        let cwd = moved.filter(|p| p.is_dir()).unwrap_or(worktree);
        Some(Self {
            workspace: workspace.to_path_buf(),
            agent_id,
            cwd,
        })
    }

    /// `dir` made workspace-relative — the pointer the §3.3 bounded-
    /// projection marker carries: `steps/<agent-id>/<NNN>/tools/
    /// <tool-id>/`, the ARCH notation, free of host paths so a committed
    /// transcript replays identically anywhere. `dir` always descends
    /// from the workspace this caller was derived from (`tool_call_dir`
    /// under `step_dir`); the fallback keeps the function total.
    pub(super) fn record_rel(&self, dir: &Path) -> PathBuf {
        dir.strip_prefix(&self.workspace)
            .unwrap_or(dir)
            .to_path_buf()
    }

    /// The env vars the harness conveys to every tool subprocess per
    /// ARCH §3.3 (the environment bullet). Names are pinned in
    /// [`super::super`] so the executor (the writer) and the built-ins that
    /// read them cannot drift; tools that do not need them ignore them.
    pub(super) fn env(&self) -> Vec<(&'static str, OsString)> {
        vec![
            (
                crate::prompt::tool::ENV_CONV_BRANCH,
                OsString::from(&self.agent_id),
            ),
            (
                crate::prompt::tool::ENV_CONV_REPO,
                self.workspace.as_os_str().to_owned(),
            ),
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::Caller;
    use std::path::{Path, PathBuf};

    fn caller(workspace: &str) -> Caller {
        Caller {
            workspace: PathBuf::from(workspace),
            agent_id: "a".into(),
            cwd: PathBuf::from(workspace).join("agents/a"),
        }
    }

    /// The marker's pointer is workspace-relative — the ARCH §2.2
    /// notation, free of host paths.
    #[test]
    fn record_rel_strips_the_workspace_prefix() {
        let rel = caller("/ws").record_rel(Path::new("/ws/steps/a/001/tools/tu_1"));
        assert_eq!(rel, Path::new("steps/a/001/tools/tu_1"));
    }

    /// Totality fallback: a dir not under the workspace (impossible via
    /// `tool_call_dir`, kept total anyway) passes through unchanged.
    #[test]
    fn record_rel_is_total_off_the_workspace() {
        let rel = caller("/ws").record_rel(Path::new("/elsewhere/tools/tu_1"));
        assert_eq!(rel, Path::new("/elsewhere/tools/tu_1"));
    }
}
