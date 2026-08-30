//! Config-commit authoring beyond `litany new` (ARCH §2.2, §2.3).
//!
//! [`super::scaffold`] authors a workspace's *first* config commit — an
//! orphan root on `config/default`. This module is the general
//! harness-assisted user act §2.2 describes for *every* later config
//! commit: materialize a transient checkout of the target config
//! lineage, refresh the `descriptions/**` snapshot from the data-root
//! pools (§3.3 — the descriptions-always producer's ongoing home), hand
//! the checkout to an edit step, commit, and tear the checkout down.
//!
//! Three [`Origin`]s cover the §2.2/§2.3 cases: **advancing** an existing
//! config branch, **forking** a new one off an existing head, and
//! starting a fresh **orphan** lineage seeded from the embedded template.
//! Only this act moves a config branch (§2.3 branch-advancement
//! invariant); an agent's governing config is derived from ancestry, so
//! a new commit here governs only agents forked after it ("fork is the
//! freeze", §2.2).
//!
//! The [`author`] core is non-interactive: the `edit` closure is the
//! seam the `litany config` bin fills with a `$EDITOR` hand-off and tests
//! fill with direct writes, so the machinery stays fully covered while
//! the untestable interactive sliver lives at the bin (ARCH §3.4).

use super::checkout::{self, Checkout};
use super::{GitRunner, TEMPLATE, descriptions};
use crate::workspace::{self, config_ref};
use std::io;
use std::path::Path;

/// How an authoring pass ended (ARCH §2.2). Both are successes: the act
/// is a *user* act, and a user who saves no change has declined it.
#[derive(Debug, PartialEq, Eq)]
pub enum Pass {
    /// The config commit landed — `config/<name>` advanced, or was
    /// created at the pass's first commit.
    Landed,
    /// The **declined pass**: the edit left the tree identical to the
    /// origin, so there was nothing to commit. Nothing was authored, the
    /// named branch did not move, and a ref the pass would have created
    /// (`--from` / `--orphan`) is not left behind — the workspace is
    /// exactly as it was, and an immediate re-author is the next thing
    /// that can happen.
    Declined {
        /// The `config/<name>` that did not move.
        target: String,
    },
}

/// Which config lineage an authoring pass targets (ARCH §2.2, §2.3).
pub enum Origin<'a> {
    /// Advance the existing `config/<name>` branch: the checkout starts
    /// at its head and the commit lands back on it.
    Advance,
    /// Fork a new `config/<name>` off the head of `config/<source>`
    /// (§2.2 "further config branches fork from existing ones").
    Fork { source: &'a str },
    /// Start `config/<name>` as a fresh orphan lineage, seeded from the
    /// embedded control-file template (§2.2 "or start fresh").
    Orphan,
}

/// Why [`author`] could not complete.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// The workspace-layout guard ([`workspace::require`]) declined the
    /// path — not a workspace, or the retired per-conversation layout.
    #[error(transparent)]
    Layout(#[from] workspace::LayoutError),
    /// Filesystem error preparing the checkout (template extraction, the
    /// checkout directory).
    #[error("config authoring I/O: {0}")]
    Io(#[source] io::Error),
    /// A `git` step failed — including the loud declines git itself
    /// raises for the illegal cases (advancing a branch that does not
    /// exist, forking onto a name that already does).
    #[error("config authoring git: {0}")]
    Git(#[source] io::Error),
    /// The `descriptions/**` refresh from the data-root pools failed
    /// (ARCH §3.3).
    #[error("descriptions-always: {0}")]
    Descriptions(#[source] descriptions::Error),
    /// The edit step (the `$EDITOR` hand-off, or a test's writer) failed.
    #[error("edit step: {0}")]
    Edit(#[source] io::Error),
    /// `--from` and `--orphan` were both given — a new branch is either a
    /// fork of a source or a fresh lineage, never both.
    #[error("pass --from <source> or --orphan, not both")]
    Conflict,
    /// `--from <source>` named a lineage the workspace does not have.
    /// Resolved *before* the transient checkout is materialized, so the
    /// decline leaves nothing behind and says nothing about git plumbing,
    /// the `.config-author` checkout, or the `config/` ref namespace the
    /// CLI otherwise hides — it names the lineages that do exist (§2.3).
    #[error(transparent)]
    NoSuchLineage(#[from] workspace::UnknownLineage),
}

/// Resolve the [`Origin`] from `litany config`'s flags and run [`author`]
/// — the testable body of the verb (ARCH §3.4). `--from` forks,
/// `--orphan` starts fresh, neither advances; both together is
/// [`Error::Conflict`]. `name` defaults to `default`. The bin supplies
/// the resolved `data_root` and the `$EDITOR` `edit` hand-off; nothing
/// here is interactive.
pub fn from_cli<G: GitRunner>(
    workspace: &Path,
    data_root: &Path,
    name: Option<&str>,
    from: Option<&str>,
    orphan: bool,
    edit: impl FnOnce(&Path) -> io::Result<()>,
    git: &G,
) -> Result<Pass, Error> {
    let origin = match (from, orphan) {
        (Some(source), false) => Origin::Fork { source },
        (None, true) => Origin::Orphan,
        (None, false) => Origin::Advance,
        (Some(_), true) => return Err(Error::Conflict),
    };
    let name = name.unwrap_or(workspace::DEFAULT_CONFIG_NAME);
    author(workspace, data_root, name, origin, edit, git)
}

/// Author one config commit onto `config/<name>` (ARCH §2.2). Guards the
/// workspace layout, resolves a fork's source lineage
/// ([`require_source`]), materializes the checkout per `origin`, refreshes
/// the `descriptions/**` snapshot from the `data_root` pools (§3.3), runs
/// `edit` against the checkout, commits, and tears the checkout down.
///
/// `edit` receives the checkout path; whatever it writes becomes the new
/// config commit's content on top of the origin's tree. An edit that
/// leaves the tree unchanged yields [`Pass::Declined`] — nothing is
/// committed and the branch does not move.
///
/// Teardown is structural, not a step: [`Checkout`] removes the checkout
/// when it drops, so the decline, a git failure, an editor failure and a
/// failed `descriptions` refresh all leave the workspace as they found it
/// — no `.config-author` to wedge the next pass, and no `config/<name>`
/// the pass created but never committed to. Whatever a *killed* pass left
/// is cleared by [`checkout::heal`] before this one materializes (§2.11).
pub fn author<G: GitRunner>(
    workspace: &Path,
    data_root: &Path,
    name: &str,
    origin: Origin,
    edit: impl FnOnce(&Path) -> io::Result<()>,
    git: &G,
) -> Result<Pass, Error> {
    workspace::require(workspace)?;
    require_source(workspace, &origin, git)?;
    let repo = workspace::repo_git(workspace);
    let author = checkout::path(workspace);
    let target = config_ref(name);

    checkout::heal(git, &repo, &author).map_err(Error::Io)?;
    let checkout = materialize(git, &repo, &target, &author, &origin)?;
    // `git worktree add` makes the dir in production; explicit for the
    // stub-git tests (a harmless no-op otherwise) — as in `scaffold`.
    std::fs::create_dir_all(&author).map_err(Error::Io)?;
    if matches!(origin, Origin::Orphan) {
        TEMPLATE.extract(&author).map_err(Error::Io)?;
    }
    descriptions::snapshot(data_root, &author).map_err(Error::Descriptions)?;
    edit(&author).map_err(Error::Edit)?;
    if !super::commit_checkout(git, &author, &commit_message(name, &origin)).map_err(Error::Git)? {
        // Dropping the guard removes the checkout and, for a fork or an
        // orphan, the ref the pass created — the decline leaves nothing.
        return Ok(Pass::Declined { target });
    }
    checkout.landed().map_err(Error::Git)?;
    Ok(Pass::Landed)
}

/// Resolve a fork's source lineage before anything is materialized — the
/// same order the agent verbs resolve `agents/<id>` in, and for the same
/// reason: a well-formed name that names nothing is the *product's*
/// decline, naming the pool it was not found in
/// ([`workspace::require_lineage`], the one home this shares with
/// `litany prompt --config`), not git's report of an invalid reference.
/// Advance and orphan name no source, so they pass through.
fn require_source<G: GitRunner>(workspace: &Path, origin: &Origin, git: &G) -> Result<(), Error> {
    let Origin::Fork { source } = origin else {
        return Ok(());
    };
    workspace::require_lineage(workspace, source, git).map_err(Error::NoSuchLineage)
}

/// Create the authoring checkout at `author` for the given origin: check
/// out the existing branch (advance), branch off a source head (fork), or
/// open a fresh orphan branch (orphan). A fork's source was resolved by
/// [`require_source`]; the remaining wrong-existence cases are git's to
/// decline (invalid reference / branch already exists). Fork and
/// orphan create `target`, so the guard owns that ref until the commit
/// lands; advance moves a branch that already exists, which a failed pass
/// must never delete.
fn materialize<'a, G: GitRunner>(
    git: &'a G,
    repo: &'a Path,
    target: &str,
    author: &Path,
    origin: &Origin,
) -> Result<Checkout<'a, G>, Error> {
    // `src` and `author_str` outlive the args they feed; `src` is empty
    // unless this is a fork.
    let src = match origin {
        Origin::Fork { source } => config_ref(source),
        _ => String::new(),
    };
    let author_str = author.to_string_lossy().to_string();
    let (args, created): (Vec<&str>, _) = match origin {
        Origin::Advance => (vec!["worktree", "add", &author_str, target], None),
        Origin::Fork { .. } => (
            vec!["worktree", "add", "-b", target, &author_str, &src],
            Some(target.to_string()),
        ),
        Origin::Orphan => (
            vec!["worktree", "add", "--orphan", "-b", target, &author_str],
            Some(target.to_string()),
        ),
    };
    Checkout::add(git, repo, author, &args, created).map_err(Error::Git)
}

/// The commit subject, naming the act and the branch it lands on — the
/// same `config: …` convention `scaffold` uses for the first commit.
fn commit_message(name: &str, origin: &Origin) -> String {
    let target = config_ref(name);
    match origin {
        Origin::Advance => format!("config: advance [{target}]"),
        Origin::Fork { source } => format!("config: fork {} [{target}]", config_ref(source)),
        Origin::Orphan => format!("config: init [{target}]"),
    }
}

#[cfg(test)]
mod tests;
#[cfg(test)]
mod tests_descriptions;
#[cfg(test)]
mod tests_lineage;
#[cfg(test)]
mod tests_stub;
#[cfg(test)]
mod tests_teardown;
