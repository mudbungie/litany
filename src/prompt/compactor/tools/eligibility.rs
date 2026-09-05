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
//! All three are read against the **removal set** — every path
//! `git rm -r` would take, not the string nominated (bl-7234) — so an
//! ancestor nomination cannot walk around a class its own member is in.
//! And all three are declined **at the nomination**, in-band, so the
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
/// and they hold for a `.md` delivery and a resumed conversation's
/// inherited first entry alike. The third cannot be: "whose output is
/// this file" is not in the name, and a blanket `summary/**` refusal
/// would forbid the supersede the chain depends on (module docs). It
/// reads the branch instead, at the one anchor the landing already
/// classifies the product by.
///
/// **The subject is the removal set, not the string** (bl-7234).
/// `mark_for_deletion` runs `git rm -r`, so a nomination of an ancestor
/// takes a not-eligible path with it while an exact-match predicate says
/// nothing: `.` sheds the system slot, `messages` sheds the dispatch
/// entry. So the two path-derived classes are read against the
/// nomination *and* against every path [`removes`] says the nomination
/// would remove, and the third is read with the nomination as a
/// pathspec, which git already resolves over the subtree. One shape for
/// all three, no ancestor/exact split: **a nomination is declined when
/// what it would remove holds a path this pass may not shed**, and the
/// decline names that path. Shedding `messages` wholesale is therefore
/// refused — the dispatch entry's reason (the operator's only copy of
/// the opening prompt, §2.7) does not weaken because the gesture was
/// coarser, and every other entry stays nominable one by one.
pub(super) fn not_compaction_eligible(
    worktree: &Path,
    agent_id: &str,
    path: &str,
    git: &dyn GitRunner,
) -> Result<Option<String>, Error> {
    let rel = path.strip_prefix("./").unwrap_or(path);
    if let Some(what) = dispatch_written(rel) {
        return Ok(Some(what.to_string()));
    }
    for member in removes(worktree, rel, git)? {
        if let Some(what) = dispatch_written(&member) {
            return Ok(Some(carries(&member, what)));
        }
    }
    if let Some(member) = written_by_this_pass(worktree, agent_id, rel, git)? {
        return Ok(Some(if member == rel {
            OWN_PRODUCT.to_string()
        } else {
            carries(&member, OWN_PRODUCT)
        }));
    }
    Ok(None)
}

/// The third class's phrase, shared by the nomination arm and the
/// removal-set arm below.
const OWN_PRODUCT: &str = "this compaction pass's own product — a file this compactor has written since \
     its own dispatch commit, which is what the landing carries forward (ARCH §2.6), \
     not history it may shed";

/// The decline's phrasing when the nomination is not itself the
/// not-eligible path but removes it. The model is told the member and
/// the rule, so it can re-nominate the siblings rather than guess which
/// file in the subtree it hit.
fn carries(member: &str, what: &str) -> String {
    format!("a nomination `git rm -r` would remove {member} with, which is {what}")
}

/// What the **dispatch wrote** — the two classes knowable from the path
/// alone, or `None` when the path is neither. Read once against the
/// nomination and once against every path the nomination removes.
fn dispatch_written(rel: &str) -> Option<&'static str> {
    if rel
        .strip_prefix(MESSAGES_DIR)
        .and_then(|r| r.strip_prefix('/'))
        .and_then(|name| name.split('-').next())
        .and_then(|nnn| nnn.parse::<u32>().ok())
        == Some(DISPATCH_SEQ)
    {
        return Some(
            "the branch's dispatch entry, its opening prompt in transcript form, \
             written at dispatch and never rewritten",
        );
    }
    if crate::prompt::dispatch::step_commit::SYSTEM_SLOT_FILES.contains(&rel)
        || rel == crate::facts::FILE
    {
        return Some(
            "one of the system slot's files (goal.md, soul.md, name) or the lineage's \
             facts.md, composed into the head of every model call (ARCH §5.2, §5.5), \
             written at dispatch and never rewritten",
        );
    }
    None
}

/// Every tracked path `git rm -r -- <rel>` would remove — the index
/// entries the nomination's pathspec matches, which for a file is that
/// file and for a directory is its whole subtree.
///
/// This is the **removal set**, and it is what the predicate judges: a
/// nomination is a gesture, and the invariant is about the files the
/// gesture takes away. `git ls-files` reads the index, which is the same
/// list `git rm` acts on, so the two can never disagree. An untracked or
/// nonexistent path answers with an empty set and falls through to the
/// nonexistent-path decline `git rm` raises itself.
fn removes(worktree: &Path, rel: &str, git: &dyn GitRunner) -> Result<Vec<String>, Error> {
    let out = git
        .run_capture(worktree, &["ls-files", "-z", "--", rel])
        .map_err(|source| Error::Git {
            op: "mark_for_deletion removal set",
            source,
        })?;
    Ok(out.split_terminator('\0').map(str::to_string).collect())
}

/// Which path *this* compaction pass has written under `rel` — the
/// first addition or rewrite in the range after the compactor's own
/// dispatch commit, or `None` when there is none.
///
/// A **directory** nomination is covered by the same call and needs no
/// arm of its own: a git pathspec naming a directory matches its whole
/// subtree, so `-- summary` answers with `summary/002.md` exactly as
/// `-- summary/002.md` does. That is why the removal set above judges
/// only the two path-derived classes — this one already reads the
/// gesture, not the string.
///
/// That range is the landing's definition of the compaction product
/// ([`super::land::base`]: "a path *added under `summary/`* is the
/// `write_summary` product"), read here against the **index** rather
/// than a commit pair, which is exactly the set `git rm` could carry
/// away: a summary already committed by its tool step and one merely
/// staged both answer the same, and an untracked one answers `None`
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
) -> Result<Option<String>, Error> {
    let Some(dispatch) = crate::prompt::role::founding_sha(worktree, "HEAD", agent_id, git)? else {
        return Ok(None);
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
    Ok(out.lines().next().map(str::to_string))
}
