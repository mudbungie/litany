//! **What an operator does with a proposal** — list, show, accept,
//! reject (`docs/DESIGN_LEARNING_LOOP.md` §3 *The operator verb*).
//!
//! Every fact here is a query. **Fresh** and **stale** are derived at
//! read time from two commits — the proposal's parent and the lineage's
//! head — and stored nowhere, so a proposal cannot be listed fresh and
//! then accepted stale on a stale field. Acceptance is a
//! **compare-and-swap fast-forward**: `git update-ref` with the parent
//! as its expected old value, which is the same test the listing
//! rendered, performed atomically by git rather than re-checked here.
//!
//! **No merge, no rebase, no ordering rule.** Two fresh proposals
//! against one tip both list fresh; the first accept moves the head, and
//! the second is stale by the same query. The remedy for a stale
//! proposal is `--reject` and the next checkpoint, never a three-way
//! merge of somebody's memory.

use super::{PROPOSAL_REF_PREFIX, proposal_ref};
use crate::template::GitRunner;
use crate::workspace::{self, CONFIG_REF_PREFIX, config_ref, repo_git};
use std::io;
use std::path::Path;

/// Why an operator's act on a proposal could not be performed.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// Layout guard decline: not a workspace (§10).
    #[error(transparent)]
    Layout(#[from] workspace::LayoutError),
    /// A well-formed id naming no staged proposal — the product's own
    /// decline, naming what there is (the "name the pool" idiom).
    #[error("no proposal {id:?} in this workspace — staged: {}", crate::name::pool(.staged))]
    Unknown { id: String, staged: Vec<String> },
    /// The lineage advanced since the reviewer read it. Names the tip it
    /// now stands at, because that is the fact that changed and the one
    /// the operator would otherwise have to go and find.
    #[error(
        "proposal {id} is stale — it is parented on {parent}, and the lineage now stands at \
         {tip}; nothing was moved. A stale proposal cannot be merged forward (its reviewer \
         read a config that no longer governs): reject it, and the next checkpoint re-derives \
         from the current tip (docs/DESIGN_LEARNING_LOOP.md §3)"
    )]
    Stale {
        id: String,
        parent: String,
        tip: String,
    },
    /// Several lineages stand on the proposal's parent — a fork whose
    /// branches have not diverged yet. Accepting would have to choose
    /// one, and choosing is the operator's (`docs/PRINCIPLES.md`
    /// *Decline illegal operations*).
    #[error(
        "proposal {id} is parented on the head of {} lineages — {}; accepting it would have to \
         choose one. Advance or delete all but one, then accept",
        .lineages.len(),
        crate::name::pool(.lineages)
    )]
    Ambiguous { id: String, lineages: Vec<String> },
    #[error("git {op}: {source}")]
    Git {
        op: &'static str,
        #[source]
        source: io::Error,
    },
}

/// One staged proposal, as the listing renders it. Every field is read
/// from refs at the moment it is asked; none is stored.
#[derive(Debug, PartialEq, Eq)]
pub struct Row {
    /// The reviewer id the branch is named for.
    pub id: String,
    /// The config lineages whose head is the proposal's parent —
    /// rendered as the pool it is, empty when the lineage moved on.
    pub lineages: Vec<String>,
    /// The parent commit, abbreviated.
    pub parent: String,
    /// Is the parent still a lineage head? Derived, never stored.
    pub fresh: bool,
    /// `git diff --shortstat` against the parent.
    pub diffstat: String,
    /// The commit subject — the reviewer's own first line.
    pub subject: String,
}

/// Every staged proposal, oldest branch name first (the ref listing's
/// order, which is the ids' own). A workspace with none yields none.
pub fn list(ws: &Path, git: &dyn GitRunner) -> Result<Vec<Row>, Error> {
    workspace::require(ws)?;
    ids(ws, git)?
        .into_iter()
        .map(|id| row(ws, &id, git))
        .collect()
}

/// The staged proposal ids — the `proposal/*` ref namespace *is* the
/// registry (§2.3), so there is nothing else to consult.
pub fn ids(ws: &Path, git: &dyn GitRunner) -> Result<Vec<String>, Error> {
    workspace::ref_names(ws, PROPOSAL_REF_PREFIX, git).map_err(|source| Error::Git {
        op: "for-each-ref proposal/",
        source,
    })
}

/// One proposal's row.
fn row(ws: &Path, id: &str, git: &dyn GitRunner) -> Result<Row, Error> {
    let target = proposal_ref(id);
    let parent = rev(ws, &format!("{target}^"), git)?;
    let lineages = heads_at(ws, &parent, git)?;
    Ok(Row {
        id: id.to_owned(),
        fresh: !lineages.is_empty(),
        lineages,
        diffstat: capture(
            ws,
            &["diff", "--shortstat", &parent, &target],
            "diff --shortstat",
            git,
        )?,
        subject: capture(
            ws,
            &["log", "-1", "--format=%s", &target],
            "log subject",
            git,
        )?,
        parent: short(&parent),
    })
}

/// The proposal's message and its whole diff — `git show`, which is
/// exactly the two things §3 says an operator reads.
pub fn show(ws: &Path, id: &str, git: &dyn GitRunner) -> Result<String, Error> {
    require_staged(ws, id, git)?;
    capture(ws, &["show", &proposal_ref(id)], "show", git)
}

/// **Accept**: fast-forward the lineage head to the proposal by
/// compare-and-swap, then delete the proposal branch. The expected old
/// value is the proposal's parent, so a lineage that moved between the
/// read and the write is refused by git itself rather than by a check
/// that could race it. Follow-the-tip (ARCH §2.2) delivers the accepted
/// patch to every agent on the lineage at its next step, with no act per
/// agent.
pub fn accept(ws: &Path, id: &str, git: &dyn GitRunner) -> Result<String, Error> {
    require_staged(ws, id, git)?;
    let target = proposal_ref(id);
    let parent = rev(ws, &format!("{target}^"), git)?;
    let commit = rev(ws, &target, git)?;
    let lineages = heads_at(ws, &parent, git)?;
    let [name] = lineages.as_slice() else {
        return Err(stale_or_ambiguous(ws, id, &parent, lineages, git));
    };
    let head = format!("refs/heads/{}", config_ref(name));
    run(
        ws,
        &["update-ref", &head, &commit, &parent],
        "update-ref accept",
        git,
    )?;
    run(
        ws,
        &["update-ref", "-d", &format!("refs/heads/{target}")],
        "update-ref -d",
        git,
    )?;
    Ok(format!(
        "accepted {id}: {} now stands at {}",
        config_ref(name),
        short(&commit)
    ))
}

/// **Reject**: delete the proposal branch and nothing else. The
/// reviewer's own branch survives as the record of its reasoning, and is
/// reaped with its dispatcher like any child (`litany delete`).
pub fn reject(ws: &Path, id: &str, git: &dyn GitRunner) -> Result<String, Error> {
    require_staged(ws, id, git)?;
    let target = proposal_ref(id);
    run(
        ws,
        &["update-ref", "-d", &format!("refs/heads/{target}")],
        "update-ref -d",
        git,
    )?;
    Ok(format!("rejected {id}: {target} deleted"))
}

/// Which decline a non-singleton lineage set is: none means the head
/// moved (stale, naming where it stands now), several mean the choice is
/// the operator's.
fn stale_or_ambiguous(
    ws: &Path,
    id: &str,
    parent: &str,
    lineages: Vec<String>,
    git: &dyn GitRunner,
) -> Error {
    if !lineages.is_empty() {
        return Error::Ambiguous {
            id: id.to_owned(),
            lineages,
        };
    }
    let tip = workspace::current_config::current_config(ws, &proposal_ref(id), git)
        .map(|r| short(r.commit()))
        .unwrap_or_else(|_| "an unreadable lineage".into());
    Error::Stale {
        id: id.to_owned(),
        parent: short(parent),
        tip,
    }
}

/// The config lineages whose head *is* `commit` — the refs a
/// fast-forward would move, which is the same set "fresh" is derived
/// from. Names, because the CAS needs one.
fn heads_at(ws: &Path, commit: &str, git: &dyn GitRunner) -> Result<Vec<String>, Error> {
    let repo = repo_git(ws);
    let listing = git
        .run_capture(
            &repo,
            &[
                "for-each-ref",
                "--format=%(objectname) %(refname:short)",
                &format!("refs/heads/{CONFIG_REF_PREFIX}"),
            ],
        )
        .map_err(|source| Error::Git {
            op: "for-each-ref config/",
            source,
        })?;
    Ok(listing
        .lines()
        .filter_map(|line| line.trim().split_once(' '))
        .filter(|(sha, _)| *sha == commit)
        .filter_map(|(_, name)| name.strip_prefix(CONFIG_REF_PREFIX))
        .map(str::to_owned)
        .collect())
}

/// Decline a well-formed id that names no staged proposal, naming the
/// ones there are — before any ref is read or written.
fn require_staged(ws: &Path, id: &str, git: &dyn GitRunner) -> Result<(), Error> {
    workspace::require(ws)?;
    let staged = ids(ws, git)?;
    if staged.iter().any(|s| s == id) {
        return Ok(());
    }
    Err(Error::Unknown {
        id: id.to_owned(),
        staged,
    })
}

/// `git rev-parse <spec>`, trimmed.
fn rev(ws: &Path, spec: &str, git: &dyn GitRunner) -> Result<String, Error> {
    capture(ws, &["rev-parse", spec], "rev-parse", git)
}

/// A capture in the workspace's bare repo, tagged with its op.
fn capture(
    ws: &Path,
    args: &[&str],
    op: &'static str,
    git: &dyn GitRunner,
) -> Result<String, Error> {
    git.run_capture(&repo_git(ws), args)
        .map(|out| out.trim().to_owned())
        .map_err(|source| Error::Git { op, source })
}

/// A write in the workspace's bare repo, tagged with its op.
fn run(ws: &Path, args: &[&str], op: &'static str, git: &dyn GitRunner) -> Result<(), Error> {
    git.run(&repo_git(ws), args)
        .map_err(|source| Error::Git { op, source })
}

/// The abbreviated commit every product prints — git's own 12, applied
/// here so the width is one fact rather than each caller's.
fn short(sha: &str) -> String {
    sha.chars().take(12).collect()
}

#[cfg(test)]
mod tests;
