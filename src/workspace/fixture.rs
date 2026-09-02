//! Shared test fixtures for the workspace physical model (ARCH §2.2):
//! a real scaffolded workspace (bare repo.git + first config commit on
//! `config/default`) and an agent branch forked off it, exactly the
//! shapes production verbs run against. Test-only (`cfg(test)` on the
//! module declaration).

use super::{DEFAULT_CONFIG_NAME, agent_ref, agent_worktree, config_ref, repo_git};
use crate::harness_root::Roots;
use crate::template::authoring::{self, Origin};
use crate::template::{GitRunner, RealGit, scaffold};
use std::path::{Path, PathBuf};
use tempfile::TempDir;

/// A real workspace under a fresh tempdir: the whole of `litany new` —
/// found the harness root, then scaffold (§2.2 *Founding is a step of
/// `litany new`*). Founding is what makes the first config commit's
/// `descriptions/**` describe the tools the shipped `providers.yaml`
/// grants (§3.3), so a fixture that skipped it would author the one
/// state `new` cannot reach. No template override. Returns
/// `(holder, workspace_path)`.
pub(crate) fn workspace() -> (TempDir, PathBuf) {
    let holder = TempDir::new().unwrap();
    let data_root = holder.path().join("data");
    std::fs::create_dir_all(&data_root).unwrap();
    let roots = Roots {
        config: holder.path().join("config"),
        data: data_root,
    };
    crate::install::prime(&roots).unwrap();
    let ws = holder.path().join("ws");
    scaffold(&ws, &roots, &RealGit::new()).unwrap();
    (holder, ws)
}

/// Fork `agents/<id>` off `start` (a config branch or another agent's
/// ref) with its worktree at `<ws>/agents/<id>`, and land a dispatch
/// commit (`goal.md`) so the tip advances past the fork point. Returns
/// the worktree path.
pub(crate) fn spawn_agent(ws: &Path, id: &str, start: &str) -> PathBuf {
    let g = RealGit::new();
    let wt = agent_worktree(ws, id);
    let wt_str = wt.to_string_lossy().to_string();
    let branch = agent_ref(id);
    g.run(
        &repo_git(ws),
        &[
            "worktree",
            "add",
            "-b",
            branch.as_str(),
            wt_str.as_str(),
            start,
        ],
    )
    .unwrap();
    // Unique goal content (the id) so a child forked off a parent's
    // tip still stages a change and the dispatch commit lands.
    std::fs::write(wt.join("goal.md"), id).unwrap();
    g.run(&wt, &["add", "goal.md"]).unwrap();
    g.run(&wt, &["commit", "-m", "dispatch"]).unwrap();
    wt
}

/// [`spawn_agent`] off the default config branch — the fresh-root shape.
pub(crate) fn spawn_root(ws: &Path, id: &str) -> PathBuf {
    spawn_agent(ws, id, &config_ref(DEFAULT_CONFIG_NAME))
}

/// Advance `config/<name>` with the given control files — the
/// harness-assisted user act of §2.2, over the shipped authoring core
/// ([`authoring::author`], `Origin::Advance`). Under follow-the-tip
/// (§2.2, bl-403b) the new head reaches every agent on the lineage at
/// its next step. Fixtures carry no data-root pools, so the
/// descriptions refresh reads an absent pool (an empty tree, §3.3) — any
/// nonexistent path serves as the `data_root`.
pub(crate) fn amend_lineage(ws: &Path, name: &str, files: &[(&str, &str)]) {
    let owned: Vec<(String, String)> = files
        .iter()
        .map(|(r, c)| (r.to_string(), c.to_string()))
        .collect();
    authoring::author(
        ws,
        &ws.join(".no-pools"),
        name,
        Origin::Advance,
        move |dir| {
            for (rel, content) in &owned {
                let path = dir.join(rel);
                std::fs::create_dir_all(path.parent().unwrap())?;
                std::fs::write(path, content)?;
            }
            Ok(())
        },
        &RealGit::new(),
    )
    .unwrap();
}

/// [`amend_lineage`] on the default lineage.
pub(crate) fn amend_config(ws: &Path, files: &[(&str, &str)]) {
    amend_lineage(ws, DEFAULT_CONFIG_NAME, files);
}
