//! The **followed config commit** — what control resolves from under
//! follow-the-tip (ARCH §2.2 *Fork chooses the lineage*, operator ruling
//! 2026-09-01, `docs/DESIGN_CONFIG_FOLLOW.md`).
//!
//! The governing config commit ([`super::governing_config`]) is the pure
//! ancestry query and does not move; until bl-403b it was also what
//! control resolved from, which pinned every conversation to the config
//! it forked off. The ruling inverts the default: a conversation
//! resolves the workspace's *current* config at every step boundary, so
//! resolution asks this module for the governing lineage's **tip** —
//! the head the operator's next `litany config` advances — and only a
//! step in flight finishes on the commit it started with.
//!
//! The derivation, one rule with no special case: take the config heads
//! whose history contains the governing commit, and collect their
//! **distinct tips**. Exactly one distinct tip — the single-lineage
//! case, and equally the freshly-forked-variant case where several
//! heads still stand on one commit — resolves that tip. Anything else
//! resolves the **governing commit itself** (the pre-ruling answer):
//! zero tips means the lineage ref is gone, two or more means real
//! divergence this derivation must not guess between — the agent stays
//! on its fork commit, the resolver says so loudly, and `litany
//! retarget` is the act that settles the lineage (§2.2).

use super::{CONFIG_REF_PREFIX, governing_config, repo_git};
use crate::template::GitRunner;
use std::io;
use std::path::Path;

/// What the derivation answered: the commit control resolves from, and
/// whether it is a followed tip or the fork commit held for want of one
/// unambiguous lineage.
#[derive(Debug, PartialEq, Eq)]
pub enum Resolution {
    /// One distinct tip stands over the governing commit: control
    /// follows the lineage — this is its current head.
    Tip(String),
    /// Not exactly one distinct tip — diverged lineages both reach the
    /// agent (`tips` ≥ 2; zero cannot occur once the governing
    /// derivation succeeded): control resolves the governing commit
    /// itself, the pre-ruling answer.
    ForkCommit { commit: String, tips: usize },
}

impl Resolution {
    /// The commit control resolves from, whichever arm answered.
    pub fn commit(&self) -> &str {
        match self {
            Resolution::Tip(commit) | Resolution::ForkCommit { commit, .. } => commit,
        }
    }

    /// `Some(distinct-tip count)` when the lineage could not be
    /// followed — the resolver's cue to say so loudly.
    pub fn held(&self) -> Option<usize> {
        match self {
            Resolution::Tip(_) => None,
            Resolution::ForkCommit { tips, .. } => Some(*tips),
        }
    }
}

/// Derive the followed config commit for `rev` (an agent ref, a config
/// head, or any commit — the same set [`governing_config`] takes).
pub fn current_config(workspace: &Path, rev: &str, git: &dyn GitRunner) -> io::Result<Resolution> {
    let governing = governing_config(workspace, rev, git)?;
    let repo = repo_git(workspace);
    let heads = git.run_capture(
        &repo,
        &[
            "for-each-ref",
            "--format=%(objectname)",
            &format!("refs/heads/{CONFIG_REF_PREFIX}"),
        ],
    )?;
    let mut tips: Vec<String> = heads
        .lines()
        .map(str::trim)
        .filter(|tip| !tip.is_empty())
        .filter(|tip| {
            git.run(&repo, &["merge-base", "--is-ancestor", &governing, tip])
                .is_ok()
        })
        .map(str::to_owned)
        .collect();
    tips.sort();
    tips.dedup();
    match tips.len() {
        1 => Ok(Resolution::Tip(tips.remove(0))),
        n => Ok(Resolution::ForkCommit {
            commit: governing,
            tips: n,
        }),
    }
}

#[cfg(test)]
mod tests;
