//! The **facts file** — `facts.md`, a config lineage's durable memory
//! (ARCH §2.7, §5.5; `docs/DESIGN_CONTEXT_ECONOMY.md` §3).
//!
//! One small artifact with consumers in three module trees — the
//! dispatch commit that cuts it, the config authoring that refuses an
//! over-cap one, the compactor door that will not shed it — so its
//! path, its cap, its cut and its refusal live here rather than in any
//! one of them (`docs/PRINCIPLES.md` single source of truth). The shape
//! [`crate::skill`] has for SKILL.md frontmatter: one format, one
//! module, producer and consumer both reading it.
//!
//! **It reaches an agent the way `descriptions/**` does.** `litany
//! config` writes it onto the config branch beside `souls/` — the one
//! user act that advances a lineage (§2.2), and the only writer, since
//! control is read from the config commit and an agent's worktree
//! writes never reach one. The dispatch commit [`cut`]s it out of the
//! **followed config commit** into the new branch's tree (§2.3 step 2),
//! and the shipped `worker` manifest pins it, so it composes as a
//! path-framed head block frozen for the branch's life (§5.5). Every
//! fork re-cuts, so a fact authored today reaches every agent forked
//! after it while no running branch's prefix moves under it.
//!
//! **The tree's copy is a function of the commit alone**, never of what
//! the fork point happened to carry — the rule
//! [`crate::prompt::dispatch::step_commit::descriptors`] states for the
//! descriptor cut beside this one. Carried in the commit: checked out
//! from it, so a child whose dispatcher's tree holds a stale copy gets
//! the followed commit's bytes. Absent there: absent in the tree, the
//! inherited copy removed rather than kept. Absent in both: nothing, at
//! no git command at all — the ordinary shape of every lineage that has
//! authored no fact, and the general path with empty inputs rather than
//! a case.
//!
//! **The cap is a refusal, not a shed.** A pinned path is never shed by
//! assembly's `budget_tokens` (§5.2), which is exactly why the ceiling
//! has to sit at the write: over-capacity fails explicitly instead of
//! silently evicting. [`MAX_BYTES`] is a constant of the artifact, not
//! a manifest key — the point of a facts file is that it is small
//! enough to be *always* present, and a workspace wanting more has
//! procedures that belong in a skill and reference material that
//! belongs in a work product the agent reads on demand.

use crate::template::GitRunner;
use std::io;
use std::path::Path;

/// The facts file's path, in a config commit and in the tree a fork
/// cuts it into — the worktree root, so the manifest's `pinned:
/// [facts.md]` rule (§5.2) sees it.
pub const FILE: &str = "facts.md";

/// The facts file's byte ceiling: about a thousand tokens at §5.2's
/// ~4 bytes/token estimate, small enough to ride the head of every
/// model call on every agent of the lineage.
pub const MAX_BYTES: u64 = 4096;

/// Why a config commit's facts file is refused at the write.
#[derive(Debug, thiserror::Error)]
pub enum OverCap {
    /// The authored file is larger than [`MAX_BYTES`]. Names both
    /// numbers, because "too large" without the ceiling leaves the
    /// author guessing at how much to cut.
    #[error(
        "{FILE} is {bytes} bytes, over the {MAX_BYTES}-byte cap — it rides the head of every \
         model call on every agent of this lineage, so it is refused at the write rather than \
         shed at assembly (ARCH §5.5); move procedure into a skill and reference material \
         into a work product the agent reads on demand"
    )]
    TooLarge {
        /// The authored file's size.
        bytes: u64,
    },
    /// The authored file exists and could not be measured. Surfaced
    /// rather than read as absence: a ceiling that answers "fine" when
    /// it could not look is not a ceiling.
    #[error("measuring {FILE}: {0}")]
    Io(#[source] io::Error),
}

/// Cut the facts file out of the governing config commit into a forked
/// tree, staged for the dispatch commit (§2.3 step 2).
///
/// `git checkout <commit> -- <path>` writes *and* stages, so the
/// dispatch commit carries the file with no second `add` — the same
/// mechanism the descriptor cut uses. The removal arm is what makes the
/// tree's copy a function of the commit rather than of the fork point;
/// it is reached only when the tree actually carries one, so the
/// ordinary lineage that has authored no fact costs no git command.
pub(crate) fn cut(worktree: &Path, config_commit: &str, git: &dyn GitRunner) -> io::Result<()> {
    if committed(worktree, config_commit, git) {
        git.run(worktree, &["checkout", config_commit, "--", FILE])
    } else if worktree.join(FILE).exists() {
        git.run(worktree, &["rm", "-q", "--ignore-unmatch", "--", FILE])
    } else {
        Ok(())
    }
}

/// Does the config commit's tree carry the facts file? (`git cat-file
/// -e`, the existence question the descriptor cut asks the same way.)
fn committed(dir: &Path, commit: &str, git: &dyn GitRunner) -> bool {
    git.run(dir, &["cat-file", "-e", &format!("{commit}:{FILE}")])
        .is_ok()
}

/// Refuse an authoring checkout whose facts file is over the cap,
/// before the config commit it would be frozen into lands.
///
/// No facts file is [`Ok`]: absence is the general path with empty
/// inputs (nothing composes, nothing errors), not a missing value.
pub(crate) fn require_within_cap(checkout: &Path) -> Result<(), OverCap> {
    let bytes = match std::fs::metadata(checkout.join(FILE)) {
        Ok(meta) => meta.len(),
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(()),
        Err(e) => return Err(OverCap::Io(e)),
    };
    if bytes > MAX_BYTES {
        return Err(OverCap::TooLarge { bytes });
    }
    Ok(())
}

#[cfg(test)]
mod tests;
