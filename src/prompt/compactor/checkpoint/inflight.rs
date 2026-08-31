//! **A compaction already in flight is a checkpoint that has fired**
//! (ARCH §2.7, the third eligibility invariant; bl-b9f0).
//!
//! The clock measures from the branch's checkpoint origin — its founding
//! commit or its last compaction base ([`super::origin`]) — and a
//! compaction that has been *dispatched* has written neither. So the next
//! step boundary computes the same span, sees the same count, and fires
//! again: two compactors were observed dispatched off one branch eight
//! seconds apart, both over substantially the same span, both a full
//! model loop at the compactor's model, and only one of them able to land
//! (the other is refused as superseded, §2.6). Nothing in the mechanism
//! bounded that at two.
//!
//! The missing fact is not stored state but a derivation that can see a
//! dispatch, and git already holds it. A compactor child's branch is
//! `<parent>-<sub-id>` ([`crate::prompt::inbox::parent_of`] is the one
//! home of that descent), its role is its dispatch commit's subject
//! ([`crate::prompt::role::derive`], the one home of an agent's role),
//! and whether it has come back is the **returned mark**
//! `refs/litany/returned/<child>` that every result deposit writes
//! ([`crate::prompt::inbox::deposit`]). Three existing single-source
//! facts, no fourth one written, and every one of them a ref or a commit
//! subject readable from the branch's own worktree.
//!
//! **Where the window closes, and what is left.** The mark lands at the
//! deposit, which is strictly before the dispatching branch lands the
//! pass — so between those two moments a boundary can still fire. It is
//! at most one step wide: the hop interprets pending child results
//! *before* it steps and runs the checkpoint *after*
//! ([`crate::prompt::dispatch::advance`]), so a compactor that returned
//! before the hop began has already landed its base and moved the clock.
//! A pass dispatched inside that window is not a runaway; it is one
//! duplicate, and the landing refuses it as superseded rather than
//! writing over the summary the other pass landed. Closing it entirely
//! would mean reading the inbox for an uninterpreted result, which is a
//! second question about the same fact, and the answer it would buy is
//! one the landing already gives.

use super::super::{COMPACTOR_ROLE, Error};
use crate::prompt::{inbox, role};
use crate::template::GitRunner;
use crate::workspace;
use std::path::Path;

/// Does `agent_id` have a compaction **in flight** — a compactor it
/// dispatched that has not deposited its result (module docs)?
///
/// Read from `worktree`, a checkout onto the workspace's object store:
/// the agent registry, the child's dispatch subject and the returned
/// mark are all refs and commits, shared by every worktree of the
/// repository, so no workspace path is needed to ask.
///
/// The role lookup runs only for a child with no returned mark, which in
/// steady state is none: every child that has come back answers on one
/// `show-ref`.
pub(super) fn compaction_in_flight(
    worktree: &Path,
    agent_id: &str,
    git: &dyn GitRunner,
) -> Result<bool, Error> {
    let ids = workspace::agent_ids_at(worktree, git).map_err(|source| Error::Git {
        op: "checkpoint in-flight for-each-ref",
        source,
    })?;
    for child in ids {
        if inbox::parent_of(&child).as_deref() != Some(agent_id) || returned(worktree, &child, git)
        {
            continue;
        }
        let child_ref = workspace::agent_ref(&child);
        if role::derive(worktree, &child_ref, &child, git)?.as_deref() == Some(COMPACTOR_ROLE) {
            return Ok(true);
        }
    }
    Ok(false)
}

/// Has `child` deposited a result — does the durable returned mark exist
/// ([`inbox::deposit::returned_ref`])? A `show-ref --verify --quiet`
/// probe; any failure, an absent ref's exit 1 included, reads as "not
/// yet", which is the safe direction here: an unreadable ref suppresses
/// a checkpoint rather than duplicating a pass.
fn returned(worktree: &Path, child: &str, git: &dyn GitRunner) -> bool {
    let mark = inbox::deposit::returned_ref(child);
    git.run(worktree, &["show-ref", "--verify", "--quiet", &mark])
        .is_ok()
}
