//! The compaction landing (ARCH §2.6, §2.7, §5.5) — **rebase-forward**,
//! the zero-downtime successor to the retired compaction merge.
//!
//! A compactor forks off the **compaction point** `P` — the dispatching
//! branch's tip at dispatch, or `HEAD~keep_recent` when the workflow
//! retains a recent tail (§6) — and nominates deletions plus a summary of
//! everything at `P`. The live agent keeps stepping past `P` the whole
//! time. When the compactor returns on a `final-response` epitaph, its
//! product lands in two moves, both by the dispatching branch's own
//! executor at a step boundary (§2.3 branch advancement):
//!
//! 1. **The compaction base.** One commit whose tree is the tree at `P`
//!    with the product applied — the nominated deletions removed, the new
//!    `summary/<NNN>.md` added — parented on the **span's lower bound**:
//!    the branch's own founding commit or its previous compaction base,
//!    whichever is newer ([`super::checkpoint::origin`] read from `P`).
//!    The **compaction span** — every commit between that bound and `P` —
//!    is thereby squashed out of the branch's history: this is the squash
//!    §5.4 prices, and what keeps a transcript-bearing repo's history from
//!    bloating. The squashed commits stay reachable from the compactor's
//!    own ref until it is retired (§9.2), so nothing is unrecoverable
//!    while the provenance lives.
//! 2. **The replay.** The branch's commits after `P` — appended transcript
//!    entries, deliveries, transfers — rebase onto the base
//!    (`git rebase --onto <base> <P>`), and the branch moves to the
//!    replayed tip. Transcript entries are one immutable file each with
//!    monotonic names (§2.3), so the replay is conflict-free by
//!    construction, with exactly three exception classes:
//!
//! - **A replayed commit rewrites a work product the compaction deleted**
//!   — a modify/delete, resolved **live-branch-wins**: the live content is
//!   staged, the compaction's deletion of that path is dropped. Lost
//!   compaction, never lost work — the same worst case the deletion-only
//!   toolset already guarantees (§2.7).
//! - **A conflict git writes markers for** (both sides carry content) —
//!   the construction was violated, and staging would commit `<<<<<<<`
//!   markup into context (§5.2). The whole landing is **declined loudly**:
//!   the rebase aborts (the branch is restored bit-for-bit),
//!   `refs/litany/conflicted/<compactor-id>` marks the compactor's tip,
//!   and the branch continues uncompacted (§2.7).
//! - **Another compaction landed since `P`** — the point is no longer
//!   reachable from the branch, or a base sits in the replay span.
//!   Replaying a squash is not a landing this compactor can have; its
//!   pass is [`LandOutcome::Superseded`]: nothing lands, nothing is
//!   marked (an overtaken pass is not a defect), and the next checkpoint
//!   trigger simply fires again.
//!
//! **Filtered by construction.** The retired merge filtered the
//! compactor's private dialog *out* of a staged tree; the base is built
//! the other way around — from the product *in*: only the deletions and
//! `summary/**` additions committed after the compactor's own dispatch
//! commit enter it ([`base`]). Its dispatch-commit prune of
//! `descriptions/**` to the empty compactor grant (§3.3), its `goal.md`/
//! `soul.md`, and its transcript never cross, structurally — the same
//! polarity as the work-product transfer's excludes (§2.6, bl-475a).
//!
//! **Cache honesty** (§5.4, §5.5). Any landing truncates the provider's
//! cached prefix at the compaction point — deletion is priced from the
//! earliest changed byte, and the base rewrites the tree at `P`.
//! Rebase-forward changes **availability**, not that price: the branch
//! never idles, and the entries after `P` survive verbatim in the tree —
//! but they sit after the truncation point and are re-sent like any
//! fresh tail.

mod base;
mod span;

use super::Error;
use crate::prompt::rebase_forward::{self, Replay, Replayed};
use crate::template::GitRunner;
use crate::workspace;
use std::path::Path;

/// Outcome of a compaction landing against the dispatching branch.
#[derive(Debug, PartialEq, Eq)]
pub enum LandOutcome {
    /// The base commit landed and every commit after the compaction point
    /// replayed on top — any work-product modify/delete resolved
    /// live-branch-wins (module docs). The branch tip is the replayed
    /// tail; the base is the §5.5 rebuild point.
    Landed,
    /// The compactor produced nothing — no nominated deletion, no summary
    /// — so there is no product to land and no base worth minting. The
    /// general path with empty inputs, not an error.
    NoOp,
    /// A compaction landed since this compactor forked, so its pass is
    /// overtaken: nothing lands, nothing is marked, and the branch's next
    /// checkpoint trigger fires afresh (module docs).
    Superseded,
    /// Git had to write conflict markers during the replay — the landing
    /// is aborted, the branch restored, and
    /// `refs/litany/conflicted/<compactor-id>` marked at the compactor's
    /// tip. Carries the offending paths for the operator-facing line.
    Conflicted(Vec<String>),
}

/// Land the returning compactor `compactor_id` into the dispatching
/// branch `parent_id`, checked out at `parent_worktree` (ARCH §2.6). The
/// checkout's `HEAD` *is* the dispatching branch (§2.3); the compaction
/// point, the span bound, and the product all derive from the two refs —
/// no sidecar state anywhere (`docs/PRINCIPLES.md` Single source of
/// truth).
pub fn land(
    parent_worktree: &Path,
    parent_id: &str,
    compactor_id: &str,
    git: &dyn GitRunner,
) -> Result<LandOutcome, Error> {
    let compactor_ref = workspace::agent_ref(compactor_id);
    let Some(span) = span::of(
        parent_worktree,
        parent_id,
        compactor_id,
        &compactor_ref,
        git,
    )?
    else {
        return Ok(LandOutcome::Superseded);
    };
    let product = base::product(parent_worktree, &span.dispatch, &compactor_ref, git)?;
    if product.is_empty() {
        return Ok(LandOutcome::NoOp);
    }
    let base = base::commit(
        parent_worktree,
        compactor_id,
        &compactor_ref,
        &span,
        &product,
        git,
    )?;
    // The replay is the shared rebase-forward move (§2.6,
    // [`crate::prompt::rebase_forward`]) — the same one the retarget
    // landing performs, differing only in the base it lands on and the
    // ref a decline marks. A compaction decline marks the *compactor*,
    // whose branch holds every byte of the pass that did not land.
    let replayed = rebase_forward::run(
        parent_worktree,
        &Replay {
            branch_id: parent_id,
            point: &span.point,
            base: &base,
            mark_id: compactor_id,
            mark_at: &compactor_ref,
        },
        git,
    )?;
    Ok(match replayed {
        Replayed::Landed => LandOutcome::Landed,
        Replayed::Conflicted(paths) => LandOutcome::Conflicted(paths),
    })
}

#[cfg(test)]
mod tests;
