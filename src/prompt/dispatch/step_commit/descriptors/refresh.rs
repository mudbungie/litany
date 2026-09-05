//! **The cut is re-made at every step boundary, not only at the fork**
//! (ARCH §3.3, §2.2 follow-the-tip; bl-37cd).
//!
//! [`super::derive`] cuts an agent's `descriptions/**` to its role's
//! grant, read from the config commit, at the dispatch commit (§2.3
//! step 2). Under fork-is-the-freeze that was the whole story: the
//! commit the cut came from could not move. Under follow-the-tip
//! (bl-403b) it moves at every step boundary, and the cut did not — so
//! a tip that *widens* a role's grant left the agent calling a tool
//! nothing in its tree describes, and a tip that *revokes* one left a
//! convincing schema on disk for a tool its wire array no longer
//! declares. That second shape is the failure the cut exists to close
//! (yog bl-55b1, `super`'s module docs) reappearing one config edit
//! later.
//!
//! **The refresh is the cut, re-run.** No second mechanism and no
//! change-detection: [`super::recut`] is idempotent and total — it
//! drops nothing when nothing is ungranted and checks out the grant
//! unconditionally — so "has the followed commit moved?" is not a
//! question this module asks. It re-cuts, and lets **git** answer
//! whether anything moved, by whether the cut dirtied `descriptions/`.
//! That is the general path with empty inputs (every unchanged
//! boundary), never a special case, and it needs no record of which
//! commit the tree was last cut from — a second home for a fact the
//! tree already carries.
//!
//! **It re-cuts but does not re-decline.** [`super::derive`] refuses a
//! grant the config commit does not describe, and that refusal is the
//! *fork's*: it lands before a branch, worktree or inbox exists. At a
//! boundary the agent already exists, and killing a running
//! conversation because an operator's edit made `providers.yaml` and
//! `descriptions/**` disagree is the very failure class follow-the-tip
//! was ruled in to fix (§2.2 — the workspace whose roles moved onto a
//! dead provider row). So the undescribed tool is simply not in the
//! tree, `tools::compose`'s intersection drops it exactly as it drops
//! any absent schema, and the operator hears an §2.11 notice naming the
//! commit, the role and the tool.
//!
//! **It commits, and it commits BEFORE the read-state capture.** The
//! wire reads descriptors off the worktree (`dispatch::tools`), but
//! replay re-assembles against `meta.json`'s `commit` (§2.10), so a
//! refresh that only touched the worktree would put bytes on the wire
//! that no replay of that step could reproduce. Committing ahead of the
//! capture keeps the read state honest — the same shape and the same
//! moment as the boundary's other landing acts, the inbox drain and the
//! child-result interpretation (§2.11 *delivery is a commit landing
//! ahead of the model call*).
//!
//! **A compactor is unaffected by construction**, which matters because
//! the compaction landing reads *deletions after the dispatch commit* as
//! the pass's product (§2.6): a compactor's grant is the empty built-in
//! pair (§2.7) and its fork already dropped every descriptor, so the
//! re-derivation drops nothing, commits nothing, and cannot manufacture
//! a nomination.

use super::super::DESCRIPTIONS_DIR;
use super::{Grant, checkout_granted, committed, drop_ungranted, schema_path};
use crate::prompt::Error;
use crate::prompt::notice::notice;
use crate::template::GitRunner;
use std::path::Path;

/// Re-derive `worktree`'s descriptor tree against the boundary's
/// resolved `grant`, committing when the derivation moved it. Answers
/// whether it committed.
pub(crate) fn refresh(
    worktree: &Path,
    agent_id: &str,
    grant: &Grant<'_>,
    git: &dyn GitRunner,
) -> Result<bool, Error> {
    // The drop reads the grant WHOLE and the check-out reads only what
    // the commit describes — the asymmetry is the point, below.
    drop_ungranted(worktree, grant.tools, git)?;
    let described = describable(worktree, grant, git);
    checkout_granted(
        worktree,
        &Grant {
            role: grant.role,
            tools: &described,
            config_commit: grant.config_commit,
        },
        git,
    )?;
    let status = git
        .run_capture(worktree, &["status", "--porcelain", "--", DESCRIPTIONS_DIR])
        .map_err(|source| Error::Git {
            op: "descriptor refresh status",
            source,
        })?;
    if status.trim().is_empty() {
        return Ok(false);
    }
    let msg = format!("descriptors: follow the config tip [{agent_id}]");
    git.run(
        worktree,
        &["commit", "-m", msg.as_str(), "--", DESCRIPTIONS_DIR],
    )
    .map_err(|source| Error::Git {
        op: "descriptor refresh commit",
        source,
    })?;
    Ok(true)
}

/// The grant, narrowed to what the config commit actually describes,
/// noticing every tool it drops.
///
/// [`super::derive`] refuses such a grant instead, and that refusal is
/// the **fork's**: it lands before a branch, worktree or inbox exists.
/// At a boundary the agent already exists, and killing a running
/// conversation because an operator's edit left `providers.yaml` and
/// `descriptions/**` disagreeing is the very failure class
/// follow-the-tip was ruled in to fix (§2.2 — the workspace whose roles
/// moved onto a dead provider row, and every conversation on it kept
/// refusing). So the undescribed tool is simply absent from the tree,
/// `dispatch::tools::compose`'s intersection drops it exactly as it
/// drops any absent schema (§3.3 *not present == not available*), and
/// the operator hears an §2.11 notice naming the commit, the role and
/// the tool.
///
/// The narrowing reaches the check-out only, never [`drop_ungranted`],
/// which reads the grant whole. **Revoked** is a tool the tip removed
/// from the role's `tools:` — it is no longer in the grant at all, so
/// the whole-grant drop takes its stale copy, which is the revoke half
/// of this ball. **Undescribed** is a different fact: the tool is still
/// granted and the commit merely fails to describe it, which is a
/// config fault, not a revocation. Deleting the tree's copy on a fault
/// would destroy the only surviving description on the strength of a
/// disagreement; the notice says so and the bytes stay.
fn describable(worktree: &Path, grant: &Grant<'_>, git: &dyn GitRunner) -> Vec<String> {
    grant
        .tools
        .iter()
        .filter(|tool| {
            committed(worktree, grant.config_commit, &schema_path(tool), git) || {
                notice!(
                    "config commit {} grants {:?} to role {:?} and describes no \
                     descriptions/tools/{}.json — the tool is not on the wire (ARCH §3.3)",
                    &grant.config_commit[..grant.config_commit.len().min(12)],
                    tool,
                    grant.role,
                    tool,
                );
                false
            }
        })
        .cloned()
        .collect()
}

#[cfg(test)]
mod tests;
