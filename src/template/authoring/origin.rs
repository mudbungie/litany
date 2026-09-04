//! What an authoring pass **targets**: the lineage or branch it lands
//! on, how its transient checkout is cut, and what its commit says
//! (ARCH §2.2, §2.3, `docs/DESIGN_LEARNING_LOOP.md` §3).
//!
//! Split from [`super`] so the routine there reads as the act it
//! performs — materialize, edit, refresh, commit, tear down — while the
//! four origins that vary it stay in one place with the ref arithmetic
//! they share.

use super::super::GitRunner;
use super::super::checkout::Checkout;
use super::Error;
use crate::workspace::{config_ref, proposal::proposal_ref};
use std::path::Path;

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
    /// Stage a **proposal** (`docs/DESIGN_LEARNING_LOOP.md` §3): one
    /// config commit on `proposal/<name>` — `name` is the reviewer's
    /// agent id — parented on `parent`, the followed config commit the
    /// reviewer read, and carrying `message` (the reviewer's terminal
    /// response) as its commit message.
    ///
    /// It is a [`Origin::Fork`] in every mechanical respect — a branch
    /// cut at a commit, created by the pass and deleted with it if the
    /// pass declines — and differs in exactly two facts: the ref
    /// namespace it lands in ([`proposal_ref`], invisible to every
    /// lineage derivation until an operator accepts it) and who wrote
    /// the commit message. That is why it rides this routine rather than
    /// a second minting path: the descriptions refresh, the `SKILL.md`
    /// parse, the pool-name collision refusal and the structural
    /// teardown are the properties a proposal needs most, and they are
    /// already here.
    Proposal {
        /// The followed config commit the proposal is parented on.
        parent: &'a str,
        /// The commit message — the reviewer's terminal response.
        message: &'a str,
    },
}

/// The branch an authoring pass lands on: `proposal/<name>` for a
/// proposal, `config/<name>` for every lineage act (§2.3 — the prefix is
/// the kind, and it is applied only at the git boundary).
pub(super) fn target_ref(name: &str, origin: &Origin<'_>) -> String {
    match origin {
        Origin::Proposal { .. } => proposal_ref(name),
        _ => config_ref(name),
    }
}

/// Create the authoring checkout at `author` for the given origin: check
/// out the existing branch (advance), branch off a source head (fork),
/// open a fresh orphan branch (orphan), or cut a proposal branch at the
/// config commit it is parented on (proposal). A fork's source was resolved by
/// [`require_source`]; the remaining wrong-existence cases are git's to
/// decline (invalid reference / branch already exists). Fork and
/// orphan create `target`, so the guard owns that ref until the commit
/// lands; advance moves a branch that already exists, which a failed pass
/// must never delete.
pub(super) fn materialize<'a>(
    git: &'a dyn GitRunner,
    repo: &'a Path,
    target: &str,
    author: &Path,
    origin: &Origin,
) -> Result<Checkout<'a>, Error> {
    // `src` and `author_str` outlive the args they feed; `src` is empty
    // unless this is a fork.
    let src = match origin {
        Origin::Fork { source } => config_ref(source),
        Origin::Proposal { parent, .. } => (*parent).to_string(),
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
        // A proposal cuts its branch at the commit it is parented on,
        // the fork's shape with a commit for a source ref.
        Origin::Proposal { .. } => (
            vec!["worktree", "add", "-b", target, &author_str, &src],
            Some(target.to_string()),
        ),
    };
    Checkout::add(git, repo, author, &args, created).map_err(Error::Git)
}

/// The commit subject, naming the act and the branch it lands on — the
/// same `config: …` convention `scaffold` uses for the first commit.
pub(super) fn commit_message(name: &str, origin: &Origin) -> String {
    let target = target_ref(name, origin);
    match origin {
        Origin::Advance => format!("config: advance [{target}]"),
        Origin::Fork { source } => format!("config: fork {} [{target}]", config_ref(source)),
        Origin::Orphan => format!("config: init [{target}]"),
        // The reviewer's own terminal response, verbatim: a proposal's
        // message is the reviewer's reasoning and the operator reads it
        // as the commit (`docs/DESIGN_LEARNING_LOOP.md` §3).
        Origin::Proposal { message, .. } => (*message).to_string(),
    }
}
