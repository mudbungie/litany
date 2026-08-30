//! Bundle and replay of an agent subtree (ARCH §9.2 *Replay and
//! archival*).
//!
//! A **run** is an agent subtree within a long-lived workspace, so the
//! archival unit follows the agent, not the workspace. [`bundle`] writes
//! the whole run as **one `git bundle` plus two slices** (§9.2): the
//! bundle carries the agent's branch, its hyphen-descendants (§2.3), and
//! the complete ancestry those refs reach — back through the dispatch
//! commits to the founding commit — while `steps/<id>*` and `inbox/<id>*`
//! (the diagnostic directories outside git, §2.2) ride alongside as plain
//! copies. [`replay`] reconstructs a **scratch workspace** from that
//! archive: it fetches every branch out of the bundle into a fresh repo,
//! materializes the subtree root's worktree, and restores the two slices.
//! Inspection is then the ordinary frontend over the scratch workspace
//! (§3.5) — **replay is not a mode** (§2.3); it is plumbing plus a verb.
//!
//! **The governing lineage rides as refs.** Control is read from the
//! governing config commit, and that commit is *derived* — the nearest
//! ancestor of the agent's branch reachable from a `config/*` ref
//! ([`workspace::governing_config`], §2.2). A bundle of the `agents/*`
//! subtree alone carries that commit as a reachable object but names no
//! `config/*` ref, so the replayed workspace has nothing to take the
//! merge-base *against* and every verb declines. The bundle therefore
//! carries the subtree's governing lineage — the `config/*` refs whose
//! history reaches it ([`workspace::config_lineage`]) — beside the
//! agent refs. Still no sidecar: the refs are the single source, and the
//! replayed repo derives its governing config by the same computation
//! over the same candidate set as the workspace it came from.

use crate::template::GitRunner;
use crate::workspace;
use slices::{SLICES, copy_dir_all, copy_matching};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

mod delete;
mod slices;

pub use delete::delete;
#[cfg(test)]
mod tests;

/// The bundle filename inside an archive directory (§9.2 "One `git
/// bundle`").
pub const BUNDLE_FILE: &str = "agents.bundle";

/// Every way [`bundle`] or [`replay`] can fail.
#[derive(Debug, thiserror::Error)]
pub enum ArchiveError {
    /// Layout guard decline (§10): not a workspace, or the retired layout.
    #[error(transparent)]
    Layout(#[from] workspace::LayoutError),
    #[error("I/O error: {0}")]
    Io(#[from] io::Error),
    #[error("git {op}: {source}")]
    Git {
        op: &'static str,
        #[source]
        source: io::Error,
    },
    #[error("no branch matches agent id {0:?} in the workspace")]
    UnknownAgent(String),
    #[error("bundle {0} not found")]
    BundleMissing(PathBuf),
    #[error("bundle names no branches")]
    EmptyBundle,
    #[error("bundle branches {0:?} share no common subtree root")]
    MalformedBundle(Vec<String>),
    #[error("replay destination {0} already exists")]
    DestExists(PathBuf),
}

/// Archive the agent subtree rooted at `agent_id` into `out_dir` (§9.2):
/// one `git bundle` of `agents/<agent_id>`, its hyphen-descendants, and
/// the subtree's governing config lineage (§2.2 — the `config/*` refs
/// whose history reaches the subtree, so the replayed workspace derives
/// its governing config exactly as this one does), plus the
/// `steps/<id>*` and `inbox/<id>*` slices.
/// The layout is guarded first ([`workspace::require`], §10) — the retired
/// layout is declined before any git op, like every verb. The bundle's
/// refs are then enumerated with `git branch --list` against the bare
/// `repo.git` (the pattern `agents/<id>` plus `agents/<id>-*` is
/// the §2.3 descent namespace); an agent id that matches no branch is
/// [`ArchiveError::UnknownAgent`].
pub fn bundle(
    ws: &Path,
    agent_id: &str,
    out_dir: &Path,
    git: &dyn GitRunner,
) -> Result<(), ArchiveError> {
    workspace::require(ws)?;
    let repo = workspace::repo_git(ws);
    let mut refs = subtree_refs(&repo, agent_id, git).map_err(|source| ArchiveError::Git {
        op: "branch --list",
        source,
    })?;
    if refs.is_empty() {
        return Err(ArchiveError::UnknownAgent(agent_id.to_owned()));
    }
    refs.extend(governing_lineage(ws, agent_id, git)?);
    fs::create_dir_all(out_dir)?;
    let bundle_path = out_dir.join(BUNDLE_FILE);
    let bundle_str = bundle_path.to_string_lossy().into_owned();
    let mut args: Vec<&str> = vec!["bundle", "create", &bundle_str];
    args.extend(refs.iter().map(String::as_str));
    git.run(&repo, &args).map_err(|source| ArchiveError::Git {
        op: "bundle create",
        source,
    })?;
    for slice in SLICES {
        copy_matching(&ws.join(slice), &out_dir.join(slice), agent_id)?;
    }
    Ok(())
}

/// Reconstruct a scratch workspace from an archive directory under
/// `scratch_base` (§9.2). Fetches every branch out of
/// `<archive>/agents.bundle` into a fresh bare `repo.git` (§2.2),
/// materializes the subtree root's worktree under `agents/`, and
/// restores the `steps/` and `inbox/` slices. Returns the scratch
/// workspace path — the frontend inspects it directly.
///
/// The scratch workspace is `<scratch_base>/<primary-id>`, where the
/// **primary id** is the shortest agent id in the bundle (every other
/// branch is one of its hyphen-descendants, §2.3). It must not already
/// exist ([`ArchiveError::DestExists`]).
pub fn replay(
    archive: &Path,
    scratch_base: &Path,
    git: &dyn GitRunner,
) -> Result<PathBuf, ArchiveError> {
    let bundle_path = archive.join(BUNDLE_FILE);
    if !bundle_path.exists() {
        return Err(ArchiveError::BundleMissing(bundle_path));
    }
    let heads = bundle_heads(archive, &bundle_path, git)?;
    let primary = primary_head(&heads)?;
    let scratch = scratch_base.join(primary);
    if scratch.exists() {
        return Err(ArchiveError::DestExists(scratch));
    }
    let repo = workspace::repo_git(&scratch);
    fs::create_dir_all(&repo)?;
    // Absolute bundle path: `-C repo.git` moves git's cwd, so a relative
    // spelling would resolve against the wrong directory.
    let bundle_abs = fs::canonicalize(&bundle_path)?;
    let bundle_arg = bundle_abs.to_string_lossy().into_owned();
    run(git, &repo, &["init", "-q", "--bare"], "init")?;
    run(
        git,
        &repo,
        &["fetch", &bundle_arg, "refs/heads/*:refs/heads/*"],
        "fetch",
    )?;
    let primary_ref = workspace::agent_ref(primary);
    let primary_wt = workspace::agent_worktree(&scratch, primary);
    let primary_wt_str = primary_wt.to_string_lossy().into_owned();
    run(
        git,
        &repo,
        &["worktree", "add", &primary_wt_str, &primary_ref],
        "worktree add",
    )?;
    for slice in SLICES {
        let src = archive.join(slice);
        if src.is_dir() {
            copy_dir_all(&src, &scratch.join(slice))?;
        }
    }
    Ok(scratch)
}

/// `litany replay` wiring (§3.4/§9.2): resolve the scratch base under the
/// data root (`replays/`, isolated by `LITANY_HOME`) and replay with
/// production git. Kept in the lib so the bin stays thin, the same
/// discipline as `prompt::inbox::cli_run`.
pub fn replay_cli(archive: &Path) -> Result<PathBuf, ArchiveError> {
    let roots = crate::harness_root::resolve().map_err(io::Error::other)?;
    replay(
        archive,
        &roots.data.join("replays"),
        &crate::template::RealGit::new(),
    )
}

/// Enumerate the subtree's branches: `agents/<agent_id>` and every
/// `agents/<agent_id>-*` hyphen-descendant (§2.3), via
/// `git branch --list` against the bare repo.git. Shared with the
/// retention delete ([`delete`]), which cuts the same subtree — so the
/// failure stays an `io::Error` and each caller tags it in its own
/// error vocabulary.
fn subtree_refs(repo: &Path, agent_id: &str, git: &dyn GitRunner) -> io::Result<Vec<String>> {
    let subtree_root = workspace::agent_ref(agent_id);
    let descendants = format!("{subtree_root}-*");
    let out = git.run_capture(
        repo,
        &[
            "branch",
            "--list",
            "--format=%(refname:short)",
            subtree_root.as_str(),
            &descendants,
        ],
    )?;
    Ok(out
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .map(str::to_owned)
        .collect())
}

/// The `config/*` refs the bundle must carry beside the subtree: the
/// governing lineage of its root ([`workspace::config_lineage`], §2.2).
/// Every hyphen-descendant forks off a commit of the root's branch
/// (§2.3), so the root's lineage is the whole subtree's.
fn governing_lineage(
    ws: &Path,
    agent_id: &str,
    git: &dyn GitRunner,
) -> Result<Vec<String>, ArchiveError> {
    let lineage =
        workspace::config_lineage(ws, &workspace::agent_ref(agent_id), git).map_err(|source| {
            ArchiveError::Git {
                op: "config lineage",
                source,
            }
        })?;
    Ok(lineage.into_iter().map(|(head, _)| head).collect())
}

/// The agent ids a bundle carries (the `refs/heads/agents/` prefix
/// stripped, §2.3), via `git bundle list-heads`. The bundle's other
/// refs — the governing config lineage — are not agents and are not
/// counted: the primary id is derived over agent refs alone.
fn bundle_heads(
    dir: &Path,
    bundle_path: &Path,
    git: &dyn GitRunner,
) -> Result<Vec<String>, ArchiveError> {
    let bundle_str = bundle_path.to_string_lossy().into_owned();
    let out = git
        .run_capture(dir, &["bundle", "list-heads", &bundle_str])
        .map_err(|source| ArchiveError::Git {
            op: "bundle list-heads",
            source,
        })?;
    Ok(out
        .lines()
        .filter_map(|l| {
            let refname = l.split_whitespace().nth(1)?;
            let short = refname.strip_prefix("refs/heads/").unwrap_or(refname);
            Some(short.strip_prefix(workspace::AGENT_REF_PREFIX)?.to_owned())
        })
        .collect())
}

/// The subtree root among `heads` (agent ids): the shortest, of which
/// every other is a hyphen-descendant (§2.3). Empty is
/// [`ArchiveError::EmptyBundle`]; heads sharing no such root is
/// [`ArchiveError::MalformedBundle`].
fn primary_head(heads: &[String]) -> Result<&str, ArchiveError> {
    let primary = heads
        .iter()
        .min_by_key(|h| h.len())
        .ok_or(ArchiveError::EmptyBundle)?;
    let prefix = format!("{primary}-");
    for h in heads {
        if h != primary && !h.starts_with(&prefix) {
            return Err(ArchiveError::MalformedBundle(heads.to_vec()));
        }
    }
    Ok(primary)
}

/// Run a fire-and-forget git op, tagging failures with `op`.
fn run(
    git: &dyn GitRunner,
    dest: &Path,
    args: &[&str],
    op: &'static str,
) -> Result<(), ArchiveError> {
    git.run(dest, args)
        .map_err(|source| ArchiveError::Git { op, source })
}
