//! The **reviewer's fresh read**: check the followed config commit's
//! workspace skills into a reviewer's forked tree at its dispatch
//! commit (`docs/DESIGN_LEARNING_LOOP.md` §2, ARCH §2.2, §3.3).
//!
//! A reviewer proposes edits to exactly two path classes — a workspace
//! skill under `skills/<name>/` and the facts document — and both live
//! in the config lineage, not on the branch it was forked off. So the
//! dispatch commit puts them in its tree, **checked out of the governing
//! config commit** rather than inherited from the fork point: the same
//! checkout the descriptor cut performs for `descriptions/**`
//! ([`super::descriptors`]), and for the same reason. A fresh read then
//! precedes every write by construction — there is no arm in which a
//! reviewer patches a stale copy, because the copy it patches was
//! written from the commit its proposal will be parented on.
//!
//! **Only the skills are read here.** The second class is the facts
//! file, and *every* fork already reads that one in from the same
//! commit ([`crate::facts::cut`], the trim's part above): a second
//! checkout keyed on the role would be a redundant path to a tree state
//! the general one already guarantees. This module named the path
//! itself while that cut was unwritten; it now owns neither the name
//! nor the read.
//!
//! It runs **after** [`super::skill_bodies`] has dropped the lineage's
//! bodies, and the pair is not a contradiction: the drop removes what
//! the *fork point* happened to carry — a parent's elected copy, which
//! may be an older version of the same name — and this writes what the
//! *commit* carries. Net, the reviewer's `skills/` is the commit's
//! bodies plus whatever the dispatching branch elected under other
//! names, which is exactly its subject.
//!
//! Keyed on the role, like every other reviewer fact, and total: a
//! lineage carrying no workspace skill checks out nothing and issues no
//! git command.

use super::descriptors::{Grant, committed};
use crate::prompt::Error;
use crate::prompt::reviewer::REVIEWER_ROLE;
use crate::template::GitRunner;
use crate::workspace::{SKILLS_DIR, proposal};
use std::path::Path;

/// Check the config commit's workspace skills into a reviewer's tree,
/// and **record which commit they were read from**
/// ([`proposal::write_read_mark`]). A no-op for every other role: the
/// checkout is the reviewer's confinement made positive — what it may
/// edit is what it can see.
///
/// The mark is written whatever the commit carries, before the checkout
/// and unconditionally: a lineage carrying no workspace skill is still
/// a lineage a reviewer may propose a *new* one against, and the
/// staleness question at landing (`docs/DESIGN_LEARNING_LOOP.md` §3
/// step 4) is about the commit's identity, not about what was in it.
pub(super) fn checkout(
    worktree: &Path,
    agent_id: &str,
    grant: &Grant<'_>,
    git: &dyn GitRunner,
) -> Result<(), Error> {
    if grant.role != REVIEWER_ROLE {
        return Ok(());
    }
    proposal::write_read_mark(worktree, agent_id, grant.config_commit, git).map_err(|source| {
        Error::Git {
            op: "mark the reviewer's read",
            source,
        }
    })?;
    if !committed(worktree, grant.config_commit, SKILLS_DIR, git) {
        return Ok(());
    }
    let args: Vec<&str> = vec!["checkout", grant.config_commit, "--", SKILLS_DIR];
    git.run(worktree, &args).map_err(|source| Error::Git {
        op: "checkout the reviewer's read",
        source,
    })
}
