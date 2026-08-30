//! Who a delete covers, and what dies with them (ARCH §9.2) — the
//! derivation half of [`super`], kept apart from the removal so the act
//! reads as: enumerate, refuse, remove.
//!
//! Every query here is a query, never a stored fact (`docs/PRINCIPLES.md`
//! *Single source of truth*): the subtree is what the id's five homes
//! remember, the pending count is an inbox listing, and liveness is the
//! §2.11 lock probe.

use super::super::slices::in_subtree;
use super::{DIRS, DeleteError};
use crate::prompt::inbox::{inbox_dir, try_acquire};
use crate::template::GitRunner;
use crate::workspace::{self, MARK_REF_ROOT};
use std::collections::BTreeSet;
use std::fs;
use std::io;
use std::path::Path;

/// The ids a delete of `root` covers: `root` and its `<id>-*`
/// hyphen-descendants (§2.3), as *any* of the id's five homes remembers
/// them — the agent refs, the three id-keyed directories, and the marks.
pub(super) fn subtree(
    ws: &Path,
    root: &str,
    marks: &[String],
    git: &dyn GitRunner,
) -> Result<Vec<String>, DeleteError> {
    let refs =
        super::super::subtree_refs(&workspace::repo_git(ws), root, git).map_err(|source| {
            DeleteError::Git {
                op: "branch --list",
                source,
            }
        })?;
    let mut ids: BTreeSet<String> = refs
        .iter()
        .filter_map(|r| r.strip_prefix(workspace::AGENT_REF_PREFIX))
        .map(str::to_owned)
        .collect();
    for dir in DIRS {
        ids.extend(entries(&ws.join(dir), root)?);
    }
    ids.extend(
        marks
            .iter()
            .filter_map(|r| r.rsplit('/').next())
            .filter(|id| in_subtree(id, root))
            .map(str::to_owned),
    );
    Ok(ids.into_iter().collect())
}

/// The names under `dir` that fall in `root`'s subtree. A missing
/// directory contributes nothing — the general path with empty inputs.
fn entries(dir: &Path, root: &str) -> io::Result<Vec<String>> {
    if !dir.is_dir() {
        return Ok(Vec::new());
    }
    let mut found = Vec::new();
    for entry in fs::read_dir(dir)? {
        let name = entry?.file_name().to_string_lossy().into_owned();
        if in_subtree(&name, root) {
            found.push(name);
        }
    }
    Ok(found)
}

/// Every `refs/litany/**` mark ref in the workspace. Enumerated by their
/// shared root rather than by the four kind-prefixes their own modules
/// spell (§2.6 conflicted, §6 budget-exhausted, §6 abandoned/notify), so
/// a mark namespace added later is reaped without editing this file.
pub(super) fn mark_refs(repo: &Path, git: &dyn GitRunner) -> Result<Vec<String>, DeleteError> {
    let out = git
        .run_capture(
            repo,
            &["for-each-ref", "--format=%(refname)", MARK_REF_ROOT],
        )
        .map_err(|source| DeleteError::Git {
            op: "for-each-ref refs/litany/",
            source,
        })?;
    Ok(out
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .map(str::to_owned)
        .collect())
}

/// Undelivered deposits in `agent_id`'s inbox (§2.11) — the count a
/// confirmation enumerates. Derived from the listing, never stored.
pub(super) fn pending(ws: &Path, agent_id: &str) -> io::Result<usize> {
    let dir = inbox_dir(ws, agent_id);
    if !dir.is_dir() {
        return Ok(0);
    }
    Ok(fs::read_dir(dir)?.count())
}

/// Decline while an executor drives `agent_id` (§2.11): a non-blocking
/// [`try_acquire`] whose *success* means nobody holds the lease (released
/// at once — probing is not driving). The lock's home is the inbox
/// directory, so a path nothing occupies is a lock nobody can hold — and
/// the probe is skipped rather than allowed to *create* the directory it
/// would then delete (a `--dry-run` writes nothing). Anything else there,
/// directory or debris, is asked of the kernel: a probe that cannot open
/// its home is a decline, never an assumption of quiescence.
pub(super) fn require_quiescent(ws: &Path, agent_id: &str) -> Result<(), DeleteError> {
    let dir = inbox_dir(ws, agent_id);
    if !dir.exists() {
        return Ok(());
    }
    let free = try_acquire(&dir)
        .map_err(|source| DeleteError::Probe {
            path: dir.clone(),
            source,
        })?
        .is_some();
    match free {
        true => Ok(()),
        false => Err(DeleteError::Driven {
            id: agent_id.to_owned(),
            lock: dir,
        }),
    }
}
