//! The `stage_proposal` action of the §6 binding interpreter
//! (`docs/DESIGN_LEARNING_LOOP.md` §3, §4) — the **reviewer's landing**,
//! beside [`super::landing`] (the compaction landing it is shaped after)
//! and [`super::flush`] (which forks the reviewer in the first place).
//!
//! It mints **one config commit on `proposal/<reviewer-id>`**, parented
//! on the followed config commit the reviewer read, whose diff is the
//! reviewer's own edits and whose message is its terminal response. The
//! commit reaches no lineage: `config/*` still advances only by an
//! operator act (`litany proposal --accept`, §3 there), and every
//! lineage derivation enumerates `refs/heads/config/` alone, so a staged
//! proposal is invisible to resolution until then.
//!
//! Five refusals and one silence, each stated where it is cheapest to
//! state:
//!
//! - **The diff is the reviewer's own commits** — its founding (dispatch)
//!   commit's tree against its terminal ref's, never `merge-base` with
//!   the dispatcher, whose history the compactor forked beside it
//!   rewrites underneath (§3 step 2).
//! - **The transcript is not part of it.** Every branch's executor
//!   commits `messages/**` (ARCH §2.3) and a compaction lands
//!   `summary/**`, so both sit in that range on every reviewer that ever
//!   spoke. They are the harness's writing, not the reviewer's, and are
//!   excluded by pathspec from the same two homes that name them
//!   ([`MESSAGES_DIR`], [`SUMMARY_DIR`]) — the §3-step-3 filter then
//!   judges what is left, which is exactly what the reviewer wrote.
//! - **Two path classes are admitted and nothing else**: `skills/<name>/**`
//!   for a name the install pool does not hold, and the durable-facts
//!   document ([`FACTS_FILE`], §4). One path outside them — a loaded
//!   pool copy, a work product, a control file — refuses the whole
//!   proposal naming the path, because a proposal is one commit and
//!   partial staging is a second shape.
//! - **Freshness is commit identity** (§3 step 4): the commit the
//!   reviewer's dispatch commit read ([`proposal::read_mark`]) must
//!   still be the lineage's tip. Otherwise the proposal is refused
//!   naming the tip, and the next checkpoint re-derives from it.
//! - **The authoring pass's own refusals ride through.** A proposal is
//!   minted by the routine `litany config` is ([`authoring::author`]),
//!   so a pool-name collision, a malformed `SKILL.md` and an over-cap
//!   facts document (`docs/DESIGN_CONTEXT_ECONOMY.md` §3 — the cap is
//!   that routine's decline, and a proposal is judged by it at proposal
//!   time rather than at acceptance, §4) are refused there, in one home,
//!   and reported here. Teardown is structural, so a refused proposal
//!   leaves no ref.
//! - **An empty proposal is silent**: no branch, no ref, no notice — the
//!   structural answer to the review prompt's bias toward finding
//!   something to save (§2).
//!
//! The return itself is **consumed, never delivered** (§3 step 7): the
//! reviewer's reasoning is on its own branch and in the proposal's
//! message, and the reviewed agent is never woken into a model call by a
//! review.

mod edits;

use super::ChildResult;
use crate::facts::FILE as FACTS_FILE;
use crate::prompt::compactor::tools::SUMMARY_DIR;
use crate::prompt::dispatch::transcript::MESSAGES_DIR;
use crate::prompt::notice::notice;
use crate::prompt::{Deps, Error, role};
use crate::template::authoring::{self, Origin, Pass};
use crate::template::descriptions;
use crate::workspace::{self, SKILLS_DIR, current_config, proposal};
use edits::{admitted, changed_paths, write_patch};
use std::path::Path;

/// What one `stage_proposal` did. Every arm but [`Staged::Empty`] is
/// news for the operator, and none is an error the hop propagates: a
/// reviewer's proposal is its own business, and a refused one must not
/// fail the dispatcher's step.
enum Staged {
    /// Minted, at this ref.
    Landed(String),
    /// The reviewer edited nothing in the two classes.
    Empty,
    /// A path outside the two classes — the whole proposal is refused.
    Outside(String),
    /// The lineage advanced since the reviewer read it.
    Stale(String),
    /// The return names no dispatch commit, or no read mark stands: not
    /// a reviewer landing at all.
    Unstageable,
    /// The authoring pass refused the commit, in its own voice.
    Declined(String),
}

/// `stage_proposal` (§3): mint the proposal, report what happened, and
/// consume the result message — in every case, exactly as the compaction
/// landing consumes its trigger: the reviewer has returned, and
/// re-reading its result would re-attempt a landing whose outcome is
/// already recorded.
pub(super) fn stage(
    workspace: &Path,
    worktree: &Path,
    cr: &ChildResult,
    deps: &Deps<'_>,
) -> Result<(), Error> {
    report(&cr.child_id, &derive(workspace, worktree, cr, deps)?);
    std::fs::remove_file(&cr.path).map_err(Error::Io)
}

/// The landing's decision, refusal-first: what is refused is refused
/// before anything is materialized, so a refusal leaves nothing behind
/// to clean up.
fn derive(
    workspace: &Path,
    worktree: &Path,
    cr: &ChildResult,
    deps: &Deps<'_>,
) -> Result<Staged, Error> {
    let git = deps.git;
    let founding = role::founding_sha(worktree, &cr.terminal_ref, &cr.child_id, git)?;
    let read = proposal::read_mark(worktree, &cr.child_id, git);
    let (Some(founding), Some(read)) = (founding, read) else {
        return Ok(Staged::Unstageable);
    };
    let tip = lineage_tip(workspace, &cr.child_id, git)?;
    if tip != read {
        return Ok(Staged::Stale(tip));
    }
    let changed = changed_paths(worktree, &founding, &cr.terminal_ref, git)?;
    let pool = descriptions::pool_names(deps.data_root);
    if let Some(outside) = changed.iter().find(|path| !admitted(path, &pool)) {
        return Ok(Staged::Outside(outside.clone()));
    }
    if changed.is_empty() {
        return Ok(Staged::Empty);
    }
    mint(workspace, worktree, cr, &founding, &read, deps)
}

/// The commit the reviewer's lineage stands at **now** — the followed
/// config commit of its own branch (ARCH §2.2), which is what the read
/// mark is compared against. A workspace whose lineages have diverged
/// answers the fork commit, exactly as resolution does, and a proposal
/// against it reads as stale by the same comparison: the landing guesses
/// at nothing.
fn lineage_tip(
    workspace: &Path,
    child_id: &str,
    git: &dyn crate::template::GitRunner,
) -> Result<String, Error> {
    let rev = workspace::agent_ref(child_id);
    current_config::current_config(workspace, &rev, git)
        .map(|resolution| resolution.commit().to_string())
        .map_err(|source| Error::Git {
            op: "proposal lineage tip",
            source,
        })
}

/// Mint the proposal through the config-authoring routine (§3 step 6):
/// a transient checkout of the parent commit, the reviewer's diff
/// applied as the edit, the `descriptions/**` refresh and every
/// `SKILL.md` parsed, the commit landing on `proposal/<reviewer-id>`.
///
/// The edit is the diff as a patch — the same scratch-patch plumbing the
/// §2.6 work-product transfer spends ([`super::super::transfer::patch_path`]).
/// An **empty patch applies nothing**, and the pass then declines
/// (nothing to commit) and deletes the ref it created: the proposal that
/// changes nothing is the authoring routine's own declined pass, not a
/// case of its own.
fn mint(
    workspace: &Path,
    worktree: &Path,
    cr: &ChildResult,
    founding: &str,
    parent: &str,
    deps: &Deps<'_>,
) -> Result<Staged, Error> {
    let git = deps.git;
    let patch = super::super::transfer::patch_path(&cr.child_id);
    let patch_str = patch.to_string_lossy().into_owned();
    write_patch(worktree, founding, &cr.terminal_ref, &patch_str, git)?;
    let empty = std::fs::metadata(&patch)
        .map(|m| m.len() == 0)
        .unwrap_or(true);
    let outcome = authoring::author(
        workspace,
        deps.data_root,
        &cr.child_id,
        Origin::Proposal {
            parent,
            message: cr.response.as_deref().unwrap_or(SILENT_REVIEWER),
        },
        |dir| {
            if empty {
                return Ok(());
            }
            git.run(dir, &["apply", &patch_str])
        },
        git,
    );
    let _ = std::fs::remove_file(&patch);
    Ok(match outcome {
        Ok(Pass::Landed) => Staged::Landed(workspace::proposal::proposal_ref(&cr.child_id)),
        Ok(Pass::Declined { .. }) => Staged::Empty,
        Err(refusal) => Staged::Declined(refusal.to_string()),
    })
}

/// The commit message of a proposal whose reviewer never spoke — a
/// `final-response` return with no response is rare but lawful (§2.6),
/// and a commit needs a subject.
const SILENT_REVIEWER: &str = "proposal: the reviewer staged edits without a closing response";

/// State the landing on stderr in the operator-notice voice (ARCH §2.11
/// — a driver's stderr is read by a program, so the prefix is the
/// contract). An empty proposal says nothing: it is the expected common
/// outcome, and a notice per checkpoint would be noise the operator
/// learns to skip.
fn report(reviewer: &str, staged: &Staged) {
    match staged {
        Staged::Empty => {}
        Staged::Landed(target) => {
            notice!("proposal [{reviewer}] staged at {target} — read it with `litany proposal`")
        }
        Staged::Outside(path) => notice!(
            "proposal [{reviewer}] refused — {path} is outside a reviewer's two \
             proposable classes (workspace skills and {FACTS_FILE}, \
             docs/DESIGN_LEARNING_LOOP.md §3); nothing was staged"
        ),
        Staged::Stale(tip) => notice!(
            "proposal [{reviewer}] refused as stale — the lineage now stands at {tip}, \
             not the commit the review read; the next checkpoint re-derives"
        ),
        Staged::Unstageable => notice!(
            "proposal [{reviewer}] refused — the return names no dispatch commit or no \
             config-read mark, so there is no base to parent a proposal on"
        ),
        Staged::Declined(refusal) => {
            notice!("proposal [{reviewer}] refused by the config-authoring pass — {refusal}");
        }
    }
}

#[cfg(test)]
mod tests;
