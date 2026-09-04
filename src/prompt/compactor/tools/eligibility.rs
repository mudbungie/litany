//! The nomination gate: which branch-relative paths a compaction pass
//! may **not** shed (ARCH §2.7).
//!
//! **What is not the branch's history is not a pass's to shed** (§2.7,
//! §2.8; bl-898f, bl-541b, bl-c7bb), and
//! [`not_compaction_eligible`] is the one predicate saying which paths
//! those are. Three classes are in it.
//!
//! The **dispatch entry** is the goal in transcript form. A goal has two
//! projections written at one dispatch and neither ever rewritten — the
//! pinned `goal.md` (§2.8) and the entry the same text was deposited as
//! through the front door (§2.11: a root's opening user message, a
//! child's dispatch message). The compactor is shown one of them — its
//! own goal quotes the dispatching branch's `goal.md` verbatim (§2.7) —
//! while the other sits in the transcript it is told to prune, so that
//! entry is the one that reads as *pure duplication* to a model
//! nominating superseded files. It was nominated and deleted in
//! practice, and what it deleted was the operator's only copy of the
//! prompt the conversation exists to serve.
//!
//! The **dispatch-written head** is the second class, and strictly
//! worse when it fires: the system slot's files — `goal.md`, `soul.md`
//! and `name` ([`crate::prompt::dispatch::step_commit::SYSTEM_SLOT_FILES`],
//! §5.2 structural wire homes) — and the lineage's `facts.md`
//! ([`crate::facts`], §5.5), cut into the tree at the same dispatch
//! commit and pinned at the head of every call after it. A compactor
//! writes its own at its dispatch commit, so a nomination of one after
//! that is a deletion inside the `dispatch..tip` range the landing
//! classifies as the compactor's product (`compactor::land`): it lands
//! as a `git rm` against the *dispatching* branch's tree, which then
//! keeps stepping with no goal, no soul, no identity line or no durable
//! memory. One class and not two: a dispatch-written fact is not the
//! branch's history to shed.
//!
//! **This pass's own product** is the third, and the one with the
//! largest blast radius (bl-c7bb). A live compactor accepted
//! `mark_for_deletion` of the `summary/001.md` it had written seconds
//! earlier through the other half of this same pair; the landing admits
//! the summary and the deletions and nothing else (§2.6), so a landing
//! carrying a `git rm` of its own summary carries away the entire span
//! it was dispatched to preserve and leaves a base holding nothing of
//! it. A **blanket** `summary/**` refusal would be wrong — superseding
//! an *earlier* pass's summary is the shipped soul's own instruction
//! (`template/souls/compactor.md`) and the only thing that keeps the
//! chain from growing without bound — so the class is *this run's*
//! output, not the directory. It is read from the same place the landing
//! reads the compaction product from ([`super::land::base`]): what
//! changed after the compactor's **own dispatch commit** is the pass's,
//! and everything at or before it is the inherited branch. One
//! definition of "the product", now read by the landing that carries it
//! and by the door that refuses to shed it.
//!
//! **The landing's own product needs no fourth class.** Since bl-e655 a
//! landing writes an **extract** beside the summary
//! (`summary/NNN.refs.md`, [`super::land::extract`]), and ARCH §2.7 puts
//! *this pass's* extract in the not-eligible class exactly as this pass's
//! summary. It is there by the same predicate and by construction: the
//! extract is derived and staged by the landing, into the *dispatching*
//! branch's base commit, so it never exists on the compactor's branch for
//! the pass to nominate. An **earlier** pass's extract does exist in the
//! inherited tree, at or before this compactor's dispatch commit, and is
//! nominable — the same distinction its summary draws, for the same
//! reason: superseding it is the compaction.
//!
//! All three are declined **at the nomination**, in-band, so the
//! compactor's summary is never premised on a deletion that did not
//! happen. Live-branch-wins is dropped at the landing instead, precisely
//! because *its* fact — a race with the live branch — is not knowable
//! when the compactor nominates (§2.6).

use super::super::Error;
use crate::prompt::dispatch::MESSAGES_DIR;
use crate::template::GitRunner;
use std::path::Path;

/// Transcript counter of the **dispatch entry** — the entry every
/// branch's opening prompt lands as (module docs, §2.3, §2.11). The
/// counter is monotonic and never reused (`dispatch::transcript`), so
/// `001` names that entry for the life of the branch.
const DISPATCH_SEQ: u32 = 1;

/// What makes the branch-relative `path` un-eligible for compaction
/// (module docs), or `None` when nothing does. The returned phrase is
/// the noun the decline names it by, so the model is told which rule it
/// hit rather than being refused anonymously.
///
/// The first two classes are derived from the path alone — the dispatch
/// entry from the same `NNN-` prefix reading `dispatch::transcript`'s
/// counter uses, the system slot from the file names the composer pins —
/// so they need no tree, no config and no state, and they hold for a
/// `.md` delivery and a resumed conversation's inherited first entry
/// alike. The third cannot be: "whose output is this file" is not in the
/// name, and a blanket `summary/**` refusal would forbid the supersede
/// the chain depends on (module docs). It reads the branch instead, at
/// the one anchor the landing already classifies the product by.
pub(super) fn not_compaction_eligible(
    worktree: &Path,
    agent_id: &str,
    path: &str,
    git: &dyn GitRunner,
) -> Result<Option<&'static str>, Error> {
    let rel = path.strip_prefix("./").unwrap_or(path);
    if rel
        .strip_prefix(MESSAGES_DIR)
        .and_then(|r| r.strip_prefix('/'))
        .and_then(|name| name.split('-').next())
        .and_then(|nnn| nnn.parse::<u32>().ok())
        == Some(DISPATCH_SEQ)
    {
        return Ok(Some(
            "the branch's dispatch entry, its opening prompt in transcript form, \
             written at dispatch and never rewritten",
        ));
    }
    if crate::prompt::dispatch::step_commit::SYSTEM_SLOT_FILES.contains(&rel)
        || rel == crate::facts::FILE
    {
        return Ok(Some(
            "one of the system slot's files (goal.md, soul.md, name) or the lineage's \
             facts.md, composed into the head of every model call (ARCH §5.2, §5.5), \
             written at dispatch and never rewritten",
        ));
    }
    if written_by_this_pass(worktree, agent_id, rel, git)? {
        return Ok(Some(
            "this compaction pass's own product — a file this compactor has written since \
             its own dispatch commit, which is what the landing carries forward (ARCH §2.6), \
             not history it may shed",
        ));
    }
    Ok(None)
}

/// Has *this* compaction pass written `rel` — is it an addition or a
/// rewrite in the range after the compactor's own dispatch commit?
///
/// That range is the landing's definition of the compaction product
/// ([`super::land::base`]: "a path *added under `summary/`* is the
/// `write_summary` product"), read here against the **index** rather
/// than a commit pair, which is exactly the set `git rm` could carry
/// away: a summary already committed by its tool step and one merely
/// staged both answer the same, and an untracked one answers `false`
/// because `git rm` refuses it anyway — the nonexistent-path decline
/// takes that case, and nothing is lost either way.
///
/// `--diff-filter=AM` excludes deletions on purpose: a path this pass
/// already staged for removal is not its product, and a re-nomination of
/// one should meet the nonexistent-path decline it has always met, not
/// this one.
///
/// No dispatch commit reachable — a tree that was never founded — is the
/// general path with empty inputs: nothing was added after a commit that
/// does not exist, so nothing is this pass's ([`crate::prompt::role::founding_sha`]
/// answers `None` the same way [`super::checkpoint::origin`] does).
fn written_by_this_pass(
    worktree: &Path,
    agent_id: &str,
    rel: &str,
    git: &dyn GitRunner,
) -> Result<bool, Error> {
    let Some(dispatch) = crate::prompt::role::founding_sha(worktree, "HEAD", agent_id, git)? else {
        return Ok(false);
    };
    let out = git
        .run_capture(
            worktree,
            &[
                "diff",
                "--cached",
                "--name-only",
                "--no-renames",
                "--diff-filter=AM",
                &dispatch,
                "--",
                rel,
            ],
        )
        .map_err(|source| Error::Git {
            op: "mark_for_deletion own-product diff",
            source,
        })?;
    Ok(!out.trim().is_empty())
}
