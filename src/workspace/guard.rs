//! The workspace guards (ARCH §2.2, §2.3) — the questions every verb
//! taking a path, an agent id, or a fork point from outside asks before
//! it does anything: *is this a workspace* ([`require`]), *does this
//! agent exist* ([`require_agent`]), *does this ref exist*
//! ([`require_ref`]), *does this config lineage exist*
//! ([`require_lineage`]). One home each, so the five id-taking verbs —
//! `message`, `advance`, `stop`, `dispatch`, `bundle` — share the rule
//! and the voice rather than each keeping a copy of both.
//!
//! The last two are the same guard for the other name a start takes:
//! §2.3 makes *any ref* a legal fork point, so `litany prompt --from`
//! and `litany dispatch --from` decline an absent one here, and
//! `litany prompt --config` / `litany config --from` decline an absent
//! config lineage by naming the lineages that do exist
//! ([`crate::name::pool`]) — the refusal carries the answer to "then
//! what may I name?".
//!
//! The existence half is not universal, and the exception says why the
//! rule holds: `litany delete` (§9.2) guards the layout and the id's
//! *shape* but admits an id no ref answers to, because absence is the
//! postcondition it establishes — the other five decline an absent agent
//! precisely because their act would silently do nothing.

use super::{GitRunner, Path, PathBuf, agent_ref, repo_git};

/// Does the agent exist? — `git rev-parse --verify refs/heads/agents/<id>`
/// against the bare repo (§2.3: the ref namespace *is* the registry, so
/// existence is a query, never a stored fact). The one home of the
/// question; the verbs ask it through [`require_agent`], `litany stop`
/// as a plain predicate (via [`crate::prompt::stop::inspector`]). A
/// non-zero exit is the answer `false`, which also covers an id git
/// refuses as a ref name.
pub fn agent_exists(workspace: &Path, agent_id: &str, git: &dyn GitRunner) -> bool {
    let refspec = format!("refs/heads/{}", agent_ref(agent_id));
    git.run(
        &repo_git(workspace),
        &["rev-parse", "--verify", "--quiet", &refspec],
    )
    .is_ok()
}

/// A well-formed agent id with no `agents/*` ref. One decline, one voice,
/// for every verb that addresses an existing agent; `reason` is the
/// verb's own clause naming *why* it needed one, so what differs between
/// verbs is the cause, never the phrasing or the remedy.
#[derive(Debug, thiserror::Error)]
#[error(
    "no agent {id:?} in this workspace — {reason}; check the id against the workspace's \
     `agents/*` refs, or start an agent with `litany prompt` / `litany dispatch`"
)]
pub struct UnknownAgent {
    id: String,
    reason: &'static str,
}

/// The existence guard every verb taking an agent id from outside runs
/// before doing anything else (§2.3) — `message` before depositing
/// (§2.11), `advance` before the lease, `dispatch` before deriving the
/// governing config of the ref its child forks off (§2.5, §2.2 — the
/// dispatching branch's own unless the dispatch named a fork point). Paired with [`require`], which
/// guards the workspace itself, this is the shared sequence README
/// promises at all five id-taking verbs.
pub fn require_agent(
    workspace: &Path,
    agent_id: &str,
    reason: &'static str,
    git: &dyn GitRunner,
) -> Result<(), UnknownAgent> {
    if agent_exists(workspace, agent_id, git) {
        return Ok(());
    }
    Err(UnknownAgent {
        id: agent_id.to_owned(),
        reason,
    })
}

/// A fork point no ref or commit in this workspace answers to (§2.3
/// *Any ref is a legal fork point*). `reason` is the verb's own clause
/// naming what it wanted the ref *for*, the shape [`UnknownAgent`] uses.
#[derive(Debug, thiserror::Error)]
#[error(
    "no ref or commit {rev:?} in this workspace — {reason}; any ref is a legal fork point \
     (ARCH §2.3): a config branch, an agent branch, or any commit of either — read them \
     with `git -C <workspace>/repo.git log --all --oneline`"
)]
pub struct UnknownRef {
    rev: String,
    reason: &'static str,
}

/// Require `rev` to name something in the workspace repository —
/// `git rev-parse --verify --quiet <rev>^{commit}` against the bare repo
/// (§2.3: the ref namespace *is* the registry, so existence is a query).
/// The `^{commit}` peel is what makes the question *fork-point* shaped:
/// a start forks off a commit, so a name that resolves to anything else
/// is declined here rather than at `git worktree add`.
pub fn require_ref(
    workspace: &Path,
    rev: &str,
    reason: &'static str,
    git: &dyn GitRunner,
) -> Result<(), UnknownRef> {
    let spec = format!("{rev}^{{commit}}");
    if git
        .run(
            &repo_git(workspace),
            &["rev-parse", "--verify", "--quiet", &spec],
        )
        .is_ok()
    {
        return Ok(());
    }
    Err(UnknownRef {
        rev: rev.to_owned(),
        reason,
    })
}

/// A config lineage the workspace does not have, and the failure of the
/// query that would have found it (§2.3). One home for the decline every
/// verb that takes a *bare lineage name* shares — `litany config --from
/// <source>`, `litany prompt --config <name>` — so both name the pool of
/// lineages that do exist instead of reporting git plumbing or the
/// `config/` prefix the CLI otherwise hides.
#[derive(Debug, thiserror::Error)]
pub enum UnknownLineage {
    #[error("no config lineage {name:?} in this workspace — existing lineages: {pool}")]
    NoSuch { name: String, pool: String },
    #[error("list config lineages: {0}")]
    Git(#[source] std::io::Error),
}

/// Require `name` to be a config lineage of this workspace (§2.3),
/// resolved *before* anything is materialized so a decline leaves
/// nothing behind.
pub fn require_lineage(
    workspace: &Path,
    name: &str,
    git: &dyn GitRunner,
) -> Result<(), UnknownLineage> {
    let names = super::config_names(workspace, git).map_err(UnknownLineage::Git)?;
    if names.iter().any(|n| n == name) {
        return Ok(());
    }
    Err(UnknownLineage::NoSuch {
        name: name.to_owned(),
        pool: crate::name::pool(&names),
    })
}

/// Layout guard failures (§2.2; pre-v1 clean break, §10).
#[derive(Debug, thiserror::Error)]
pub enum LayoutError {
    /// The retired per-conversation layout: a `root/` primary worktree
    /// with untracked control files at the repo root. Pre-v1 there is
    /// no migration (§10 clean-break note): the refusal is loud and
    /// names both what was found and what the current layout is.
    #[error(
        "{0} uses the retired per-conversation layout (a `root/` primary worktree with \
         control files at the repo root); the current layout is one repo per workspace — \
         `<workspace>/repo.git` (bare) with `config/*` branches and `agents/*` refs \
         (ARCH §2.2). Pre-v1 clean break (§10): no migration — create a fresh workspace \
         with `litany new` and re-author its config"
    )]
    OldLayout(PathBuf),
    /// No `repo.git` and no old-layout signature: not a workspace.
    #[error("{0} is not a workspace (no repo.git) — create one with `litany new` (ARCH §2.2)")]
    NotAWorkspace(PathBuf),
}

/// Require `workspace` to be a current-layout workspace: `repo.git`
/// present. The retired per-conversation layout is refused with a
/// clear, actionable error (pre-v1 clean break); anything else is not
/// a workspace at all.
pub fn require(workspace: &Path) -> Result<(), LayoutError> {
    if repo_git(workspace).is_dir() {
        return Ok(());
    }
    if workspace.join("root").join(".git").exists() {
        return Err(LayoutError::OldLayout(workspace.to_path_buf()));
    }
    Err(LayoutError::NotAWorkspace(workspace.to_path_buf()))
}
