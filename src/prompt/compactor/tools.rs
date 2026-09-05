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
//! Which paths a pass may *not* shed — and why each class is in the set
//! — is [`eligibility`]'s, the one predicate `mark_for_deletion`
//! consults before it stages anything.
//!
mod eligibility;

use super::Error;
use crate::template::GitRunner;
use eligibility::not_compaction_eligible;
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
            what,
        });
    }
    git.run(worktree, &["rm", "-r", "-q", "--", path])
        .map_err(|source| Error::Git {
            op: "mark_for_deletion rm",
            source,
        })
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
#[cfg(test)]
mod tests_removal_set;
