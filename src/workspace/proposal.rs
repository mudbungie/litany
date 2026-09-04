//! The **proposal** ref namespace — `proposal/<reviewer-id>`
//! (`docs/DESIGN_LEARNING_LOOP.md` §3, ARCH §2.3).
//!
//! A proposal is one config commit a reviewer's landing minted off the
//! followed config commit it read, parked on a branch of its own until
//! an operator accepts it onto the lineage or rejects it. The workspace
//! therefore holds a **third** ref namespace beside `config/*` and
//! `agents/*`, and which advancement rule a ref lives under is derived
//! from its prefix and recorded nowhere else (§2.3): a `proposal/*`
//! branch is written once, by the reviewer's dispatcher's executor, and
//! advanced by nobody.
//!
//! It is deliberately *not* `config/*`. Every lineage derivation in the
//! workspace — the governing config, the followed tip
//! ([`super::current_config`]), the `--from` source pool — enumerates
//! `refs/heads/config/`, so a staged proposal is invisible to
//! resolution until acceptance fast-forwards a lineage onto it. Nothing
//! had to learn to ignore it.

mod ops;
mod render;

pub use ops::{Row, accept, list, reject, show};
pub use render::render;

use super::MARK_REF_ROOT;
use crate::template::GitRunner;
use std::io;
use std::path::Path;

/// Ref-namespace prefix for a staged proposal: `proposal/<reviewer-id>`
/// (§3). The bare reviewer id is the vocabulary `litany proposal` takes
/// on its command line; the prefix is applied only at the git boundary,
/// exactly as [`super::config_ref`] applies its own.
pub const PROPOSAL_REF_PREFIX: &str = "proposal/";

/// The proposal branch ref for one reviewer, `proposal/<reviewer-id>`.
pub fn proposal_ref(reviewer_id: &str) -> String {
    format!("{PROPOSAL_REF_PREFIX}{reviewer_id}")
}

/// Ref-namespace prefix of the reviewer's **read mark**,
/// `refs/litany/config-read/<reviewer-id>` — the per-agent mark
/// namespace ([`MARK_REF_ROOT`], §2.2) beside `retarget`, `cwd` and the
/// rest, so it is reaped with the agent by `litany delete` (§9.2
/// enumerates the mark root) and crosses no fork.
pub const READ_REF_PREFIX: &str = "config-read/";

/// `refs/litany/config-read/<reviewer-id>` — the mark naming the
/// **followed config commit a reviewer's dispatch commit read**
/// (`docs/DESIGN_LEARNING_LOOP.md` §2, §3 step 4).
///
/// **The mark names a commit, and that commit is exactly the fact** —
/// the same shape [`super::retarget`] takes, for the same reason: the
/// landing reads a commit-ish and nothing decodes anything, and `git gc`
/// keeps the commit alive for as long as the mark does.
///
/// It is not derivable. A reviewer's fresh read is a *checkout* of the
/// followed tip into its forked tree (§2, the dispatch commit's reviewer
/// read), which leaves no ancestry between the two commits — and
/// freshness at landing is a question about *commit identity*, never
/// about whether a patch still applies (§3 step 4). So the commit that
/// performed the read states what it read, once, here.
pub fn read_ref(reviewer_id: &str) -> String {
    format!("{MARK_REF_ROOT}{READ_REF_PREFIX}{reviewer_id}")
}

/// Record the config commit a reviewer's dispatch commit read, at the
/// mark above. Run from the reviewer's own worktree: refs are shared by
/// every worktree of a repository, so the write needs no workspace path
/// and no second git home ([`crate::prompt::workflow_actions`]'s ref
/// marks are written the same way).
pub fn write_read_mark(
    worktree: &Path,
    reviewer_id: &str,
    commit: &str,
    git: &dyn GitRunner,
) -> io::Result<()> {
    git.run(worktree, &["update-ref", &read_ref(reviewer_id), commit])
}

/// The config commit a reviewer read, or `None` when no mark stands —
/// which is every non-reviewer agent, not an error. An unreadable mark
/// reads the same way, and the landing that asks says so rather than
/// staging against a commit it cannot name.
pub fn read_mark(worktree: &Path, reviewer_id: &str, git: &dyn GitRunner) -> Option<String> {
    let spec = format!("{}^{{commit}}", read_ref(reviewer_id));
    let out = git
        .run_capture(worktree, &["rev-parse", "--verify", &spec])
        .ok()?;
    let sha = out.trim();
    (!sha.is_empty()).then(|| sha.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::template::RealGit;
    use crate::workspace::fixture;

    #[test]
    fn the_prefixes_are_applied_at_the_git_boundary() {
        assert_eq!(proposal_ref("p1-c2"), "proposal/p1-c2");
        assert_eq!(read_ref("p1-c2"), "refs/litany/config-read/p1-c2");
    }

    #[test]
    fn the_mark_round_trips_the_commit_it_named() {
        let (_h, ws) = fixture::workspace();
        let wt = fixture::spawn_root(&ws, "20260101-m1");
        let git = RealGit::new();
        let head = git.run_capture(&wt, &["rev-parse", "HEAD"]).unwrap();
        let head = head.trim();
        assert_eq!(read_mark(&wt, "20260101-m1", &git), None, "no mark yet");
        write_read_mark(&wt, "20260101-m1", head, &git).unwrap();
        assert_eq!(
            read_mark(&wt, "20260101-m1", &git).as_deref(),
            Some(head),
            "the mark names the commit"
        );
    }
}
