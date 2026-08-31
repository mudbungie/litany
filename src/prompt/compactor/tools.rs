//! Compactor toolset (ARCH §2.7) — `write_summary` and
//! `mark_for_deletion`, and nothing else.
//!
//! These are the **two** tools a compactor agent may call, and they are
//! **built into the primitive, not declared in `providers.yaml`** (§2.7):
//! the compactor's toolset is this fixed pair, injected by the harness for
//! the compactor role alone, never assembled from a role's `tools:` list.
//! The narrowness is the point — giving the compactor no general
//! filesystem write surface makes "deletion-only" a **structural**
//! property rather than a disciplinary one: the worst failure mode is lost
//! information, never corrupted information (§2.7, §2.6 live-branch-wins).
//!
//! - [`write_summary`] writes `summary/<NNN>.md` on the compactor branch —
//!   the one location it may create, picked by scanning the directory.
//! - [`mark_for_deletion`] nominates a file for removal; the harness
//!   applies the deletion at commit time. "Applied at commit time" is
//!   realized by staging the removal (`git rm`) so the compactor step's
//!   own commit carries it (§2.3), and the compaction landing (§2.6)
//!   then applies it to the base — subject to live-branch-wins on any
//!   work-product overlap in the replay.
//!
//! The deletions are **deletion-only structural**: `git rm` can remove but
//! never write content, so a compactor cannot corrupt a work product even
//! by defect. The compactor decides relevance against the dispatching
//! branch's goal (`goal.md`), which its inherited worktree carries (§2.7).
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
//! The **system slot's files** — `goal.md`, `soul.md` and `name`
//! ([`crate::prompt::dispatch::step_commit::SYSTEM_SLOT_FILES`], §5.2
//! structural wire homes) — are the other class, and strictly worse
//! when they fire. A compactor writes its own three at its dispatch
//! commit, so a nomination of one after that is a deletion inside the
//! `dispatch..tip` range the landing classifies as the compactor's
//! product (`compactor::land`): it lands as a `git rm` against the
//! *dispatching* branch's tree, which then keeps stepping with no goal,
//! no soul or no identity line on every later model call.
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
//! All three are declined **at the nomination**, in-band, so the
//! compactor's summary is never premised on a deletion that did not
//! happen. Live-branch-wins is dropped at the landing instead, precisely
//! because *its* fact — a race with the live branch — is not knowable
//! when the compactor nominates (§2.6).

use super::Error;
use crate::prompt::dispatch::MESSAGES_DIR;
use crate::template::GitRunner;
use std::path::Path;

/// Built-in tool name: write the next `summary/<NNN>.md` (ARCH §2.7).
pub(crate) const WRITE_SUMMARY: &str = "write_summary";
/// Built-in tool name: nominate a branch-relative path for removal
/// (deletion-only structural, ARCH §2.7).
pub(crate) const MARK_FOR_DELETION: &str = "mark_for_deletion";

/// Branch-relative directory holding compaction summaries (ARCH §2.7).
/// Lives at the worktree root so the manifest's role-keyed `pinned:
/// [summary/**]` rule (§5.2) sees it.
pub(crate) const SUMMARY_DIR: &str = "summary";
/// Width of the zero-padded summary-seq in summary filenames
/// (`001.md`, `002.md`). Matches the step-seq width (§2.3) so the two
/// on-disk layouts read uniformly.
const SUMMARY_SEQ_WIDTH: usize = 3;
/// Transcript counter of the **dispatch entry** — the entry every
/// branch's opening prompt lands as (module docs, §2.3, §2.11). The
/// counter is monotonic and never reused (`dispatch::transcript`), so
/// `001` names that entry for the life of the branch.
const DISPATCH_SEQ: u32 = 1;

/// Write `summary/<NNN>.md` on `worktree`, picking the next-available
/// seq by scanning the directory. Returns the branch-relative path of the
/// written file for the subsequent `git add`.
///
/// Seq is branch-global over the summary directory's contents: a branch
/// may compact several times (§2.7), and reading existing seqs here means
/// every checkpoint shares one numbering rule.
pub(crate) fn write_summary(worktree: &Path, content: &str) -> std::io::Result<String> {
    let dir_abs = worktree.join(SUMMARY_DIR);
    std::fs::create_dir_all(&dir_abs)?;
    let seq = next_seq(&dir_abs)?;
    let file_name = format!("{seq:0width$}.md", width = SUMMARY_SEQ_WIDTH);
    let path_abs = dir_abs.join(&file_name);
    std::fs::write(&path_abs, content)?;
    Ok(format!("{SUMMARY_DIR}/{file_name}"))
}

/// Nominate the branch-relative `path` for removal (ARCH §2.7). Realized
/// as `git rm -r -- <path>` inside the compactor `worktree`, staging the
/// deletion so the compactor step's commit carries it (§2.3) — the
/// "applied at commit time" contract. **Deletion-only structural**: this
/// can only remove, never write, so a compactor cannot corrupt content.
///
/// Two kinds of nomination are **declined loudly** rather than silently ignored
/// (`docs/PRINCIPLES.md` "Decline illegal operations"), and the decline
/// reaches the model in-band as an `is_error` `tool_result` (§3.3):
///
/// - a path [`not_compaction_eligible`] names — the branch's dispatch
///   entry, one of the system slot's files, or this pass's own product
///   (module docs, §2.7);
/// - a path that does not exist on the branch — a compactor nominating a
///   nonexistent file is a defect worth surfacing, and `git rm` errors on it.
///
/// `agent_id` is the compactor's own branch (harness-derived, §3.3): the
/// third class is read against that branch's dispatch commit, which is
/// the only thing that separates the pass's own output from the tree it
/// inherited.
pub(crate) fn mark_for_deletion(
    worktree: &Path,
    agent_id: &str,
    path: &str,
    git: &dyn GitRunner,
) -> Result<(), Error> {
    if let Some(what) = not_compaction_eligible(worktree, agent_id, path, git)? {
        return Err(Error::NotCompactionEligible {
            path: path.to_string(),
            what: what.to_string(),
        });
    }
    git.run(worktree, &["rm", "-r", "-q", "--", path])
        .map_err(|source| Error::Git {
            op: "mark_for_deletion rm",
            source,
        })
}

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
fn not_compaction_eligible(
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
    if crate::prompt::dispatch::step_commit::SYSTEM_SLOT_FILES.contains(&rel) {
        return Ok(Some(
            "one of the system slot's files (goal.md, soul.md, name), \
             a structural wire home composed into every model call on the branch (ARCH §5.2), \
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

/// Pick the next summary-seq: one more than the highest existing
/// `<NNN>.md` file in the directory. Non-`.md` files and files whose
/// stems don't parse as integers are skipped so an operator-dropped
/// note never fouls numbering.
fn next_seq(dir: &Path) -> std::io::Result<u32> {
    let mut max = 0u32;
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let name = entry.file_name();
        let Some(stem) = Path::new(&name).file_stem().and_then(|s| s.to_str()) else {
            continue;
        };
        if Path::new(&name).extension().and_then(|e| e.to_str()) != Some("md") {
            continue;
        }
        if let Ok(n) = stem.parse::<u32>() {
            max = max.max(n);
        }
    }
    Ok(max + 1)
}

#[cfg(test)]
mod tests;
#[cfg(test)]
mod tests_own_product;
