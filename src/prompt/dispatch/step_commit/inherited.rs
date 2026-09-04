//! Prune the fork point's **inherited dialog** from a *child's* forked
//! tree, at its dispatch commit (ARCH §2.2, §2.3 step 2, §2.5).
//!
//! `messages/**`, `summary/**` and `skills/**` are branch-scoped context
//! (§2.2): written per-branch, never meant to become any other agent's
//! context. The work-product transfer enforces that child-to-parent
//! (§2.6, `transfer`'s excludes); this prune is the same invariant
//! parent-to-child, at the one place a tree crosses branches — the fork
//! ([`crate::prompt::subagent::spawn_subagent_branch`], every child
//! dispatch; never the root path, below).
//!
//! Without it, a child forked off its parent's tip carried the parent's
//! whole conversation as its own: assembly is a pure function of the
//! tree (§5.1) and the transcript composes unconditionally (§5.2), so
//! the parent's dialog — including user-role instructions addressed to
//! the *parent* — opened every child's first model call. The reproduced
//! runaway (litany bl-5a36, from yog bl-d023): a user told an agent
//! "spawn a subagent to analyze …"; the dispatched child inherited that
//! instruction as an apparently unanswered user message — the unsettled
//! prune ([`super::unsettled`]) had deleted the parent's `dispatch`
//! `tool_use`, the only evidence the spawn already happened — and it
//! outranked the child's own deposited goal, so the child obeyed it,
//! and every generation re-dispatched until the operator stopped the
//! tree. A child's opening context is its pinned goal, soul and pins
//! plus what is deposited to it — never its dispatcher's dialog.
//!
//! **Three principled exceptions, one axis: whose conversation is it?**
//!
//! - **A fork-back-in root keeps the dialog — it *is* its
//!   conversation.** `litany prompt --from <ref>` re-enters a recorded
//!   conversation at any commit (§7.2): the inherited transcript is the
//!   very thing being resumed. That is why this prune is a part of the
//!   child spawn, not of [`super::trim_to_context`] — the root's
//!   dispatch commit must not run it.
//! - **A compactor keeps the dialog — it is its *subject*.** Its goal
//!   is to compact the dispatching branch's history, delivered by fork
//!   inheritance (§2.7: "its worktree carries that branch's transcript
//!   … and it is composed from that tree"); the summary chain must stay
//!   visible or a superseding summary destroys signal, and spent
//!   `skills/**` bodies must stay nominable for deletion. The exception
//!   is keyed on the role — the same signal the toolset injection and
//!   checkpoint exclusion already branch on (§2.7).
//! - **A reviewer keeps the dialog — it is equally its *subject***
//!   (ARCH §2.2 *Branch-scoped vs inherited*,
//!   `docs/DESIGN_LEARNING_LOOP.md` §2). It is forked at the same
//!   checkpoint, off the same compaction point, to inspect the very
//!   span the compactor beside it is about to squash away: a reviewer
//!   handed no dialog would review nothing. Same axis, same key, no
//!   second mechanism — the role is the whole of the test.
//!
//! Total like the trim's parts: a child forked off a config commit
//! carries none of these paths, and `--ignore-unmatch` makes the
//! removal a no-op there rather than a special case.

use crate::prompt::Error;
use crate::template::GitRunner;
use std::path::Path;

/// The branch-scoped dialog paths a child fork must not carry across
/// (§2.2): the transcript ([`crate::prompt::dispatch::MESSAGES_DIR`]),
/// the summary chain, the loaded skill bodies. `goal.md`, `soul.md` and
/// `name` complete the §2.6 branch-scoped set but are overwritten by
/// the dispatch commit itself, so they need no removal here.
const DIALOG_PATHS: &[&str] = &[crate::prompt::dispatch::MESSAGES_DIR, "summary", "skills"];

/// The child roles whose *subject* is the dispatching branch's own
/// dialog, so their fork keeps it (module docs). The third keeper, a
/// fork-back-in root, is not a role but a path: it never reaches here.
const DIALOG_KEEPERS: [&str; 2] = [
    crate::prompt::compactor::COMPACTOR_ROLE,
    crate::prompt::reviewer::REVIEWER_ROLE,
];

/// Stage the removal of the fork point's inherited dialog — for every
/// child role but the keepers, whose subject it is (module docs).
pub(crate) fn prune_inherited_dialog(
    worktree: &Path,
    role: &str,
    git: &dyn GitRunner,
) -> Result<(), Error> {
    if DIALOG_KEEPERS.contains(&role) {
        return Ok(());
    }
    let mut args: Vec<&str> = vec!["rm", "-r", "-q", "--ignore-unmatch", "--"];
    args.extend_from_slice(DIALOG_PATHS);
    git.run(worktree, &args).map_err(|source| Error::Git {
        op: "rm inherited dialog",
        source,
    })
}

#[cfg(test)]
mod tests;
