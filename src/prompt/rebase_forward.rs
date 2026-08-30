//! **Rebase-forward** — the one landing move in the system (ARCH §2.6):
//! mint a base commit, then replay every commit after a boundary commit
//! onto it and move the branch to the replayed tip. This module is the
//! *replay* half, shared by the two landings that perform it:
//!
//! - the **compaction landing** ([`super::compactor::land`]), whose base
//!   is the compaction point's tree with the product applied, parented on
//!   the span's lower bound;
//! - the **retarget landing** ([`super::retarget`]), whose base is a
//!   re-derived dispatch commit parented on the target config commit.
//!
//! Both replay with `git rebase --empty=keep --onto <base> <point>
//! <branch>`: it re-lands each commit in `point..tip` in order (keeping
//! ones the new base made empty — a delete/delete agreement is still a
//! commit the checkpoint clock counts), then points the branch at the
//! result. Transcript entries are one immutable file each with monotonic
//! names (§2.3), so the replay is conflict-free by construction; where git
//! stops anyway, the index stages say which of the two legal exception
//! classes this is (the same stage-reading discipline as the retired
//! merge's decline, bl-a9eb):
//!
//! - **stages 1+3 only** — a modify/delete: the replayed commit rewrote a
//!   work product the base does not carry. Git leaves the live content in
//!   the worktree; staging it (`git add`) resolves **live-branch-wins**,
//!   dropping the base's deletion. Lost landing, never lost work.
//! - **anything else** — stage 2 present (both sides carry content — git
//!   wrote `<<<<<<<` markers) or a shape the construction does not admit:
//!   the landing is **declined loudly**. `git rebase --abort` restores
//!   the branch bit-for-bit, `refs/litany/conflicted/<mark-id>` is marked
//!   at the ref the caller names, and nothing lands (§2.6 decline — the
//!   same escape hatch as the work-product transfer).

use super::Error;
use crate::template::GitRunner;
use crate::workspace::CONFLICTED_REF_PREFIX;
use std::collections::BTreeMap;
use std::path::Path;

/// How a replay ended. The caller maps it into its own landing outcome —
/// the two landings have different *non*-replay arms (a superseded
/// compaction, a no-op retarget) and share only these two.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Replayed {
    /// Every commit after the point replayed onto the base and the branch
    /// moved to the replayed tip — any work-product modify/delete
    /// resolved live-branch-wins (module docs).
    Landed,
    /// Git had to write conflict markers: the rebase is aborted, the
    /// branch restored, and `refs/litany/conflicted/<mark-id>` marked.
    /// Carries the offending paths for the operator-facing line.
    Conflicted(Vec<String>),
}

/// One replay: which branch moves, from which boundary, onto which base,
/// and where a decline's mark goes. Held as a struct because the five are
/// meaningless apart — a replay is exactly this tuple (`docs/PRINCIPLES.md`
/// minimal interface).
pub(crate) struct Replay<'a> {
    /// The agent whose branch (`agents/<id>`) the replay moves.
    pub(crate) branch_id: &'a str,
    /// The boundary commit: commits *after* it are what replays.
    pub(crate) point: &'a str,
    /// The freshly minted base the tail lands on.
    pub(crate) base: &'a str,
    /// Mark name for a decline — `refs/litany/conflicted/<mark_id>`. The
    /// compaction landing marks the *compactor*, whose branch holds the
    /// work at risk; the retarget landing marks the agent itself.
    pub(crate) mark_id: &'a str,
    /// Where that mark points: the ref preserving every byte of the work
    /// a decline leaves unlanded.
    pub(crate) mark_at: &'a str,
}

/// Replay the branch's commits after `point` onto `base` and move the
/// branch to the replayed tip (module docs). The loop is bounded by the
/// number of commits being replayed: each continue settles at least one,
/// so more stops than commits means git is not making progress and the
/// landing aborts rather than spins.
pub(crate) fn run(
    worktree: &Path,
    replay: &Replay<'_>,
    git: &dyn GitRunner,
) -> Result<Replayed, Error> {
    let branch = crate::workspace::agent_ref(replay.branch_id);
    let range = format!("{}..HEAD", replay.point);
    let stops = git
        .run_capture(worktree, &["rev-list", "--count", &range])
        .map_err(|source| Error::Git {
            op: "rebase-forward replay count",
            source,
        })?
        .trim()
        .parse::<u32>()
        .unwrap_or(0);

    let mut result = git.run(
        worktree,
        &[
            "rebase",
            "--empty=keep",
            "--onto",
            replay.base,
            replay.point,
            &branch,
        ],
    );
    let mut budget = stops;
    while let Err(source) = result {
        let unmerged = unmerged_stages(worktree, git)?;
        let keep: Vec<&String> = unmerged
            .iter()
            .filter(|(_, s)| **s == (true, false, true))
            .map(|(path, _)| path)
            .collect();
        let marked: Vec<String> = unmerged
            .keys()
            .filter(|path| !keep.contains(path))
            .map(String::clone)
            .collect();
        if unmerged.is_empty() || budget == 0 {
            // Not a conflict stop (a dirty tree, a bad ref) — or git is
            // not making progress. Restore the branch and surface the
            // rebase's own failure.
            let _ = git.run(worktree, &["rebase", "--abort"]);
            return Err(Error::Git {
                op: "rebase-forward rebase",
                source,
            });
        }
        if !marked.is_empty() {
            return decline(worktree, replay, marked, git);
        }
        budget -= 1;
        let mut add = vec!["add", "--"];
        add.extend(keep.iter().map(|s| s.as_str()));
        git.run(worktree, &add).map_err(|source| Error::Git {
            op: "rebase-forward live-branch-wins add",
            source,
        })?;
        result = git.run(
            worktree,
            &["-c", "core.editor=true", "rebase", "--continue"],
        );
    }
    Ok(Replayed::Landed)
}

/// Unmerged paths and which index stages each populates — `(base, ours,
/// theirs)`, i.e. stages 1/2/3 of `git ls-files -u`. In a rebase stop,
/// "ours" is the base side being rebased onto and "theirs" is the live
/// commit being replayed.
fn unmerged_stages(
    worktree: &Path,
    git: &dyn GitRunner,
) -> Result<BTreeMap<String, (bool, bool, bool)>, Error> {
    let out = git
        .run_capture(worktree, &["ls-files", "-u"])
        .map_err(|source| Error::Git {
            op: "rebase-forward unmerged",
            source,
        })?;
    let mut stages: BTreeMap<String, (bool, bool, bool)> = BTreeMap::new();
    for line in out.lines() {
        // `<mode> <sha> <stage>\t<path>`
        let Some((meta, path)) = line.split_once('\t') else {
            continue;
        };
        let entry = stages.entry(path.to_string()).or_default();
        match meta.rsplit(' ').next() {
            Some("1") => entry.0 = true,
            Some("2") => entry.1 = true,
            Some("3") => entry.2 = true,
            _ => {}
        }
    }
    Ok(stages)
}

/// Refuse the landing loudly (§2.6 decline): abort the rebase so the
/// branch and its worktree are exactly as they were, mark
/// `refs/litany/conflicted/<mark-id>` at the ref the caller named — every
/// byte of the work preserved for the operator — and land nothing.
fn decline(
    worktree: &Path,
    replay: &Replay<'_>,
    paths: Vec<String>,
    git: &dyn GitRunner,
) -> Result<Replayed, Error> {
    git.run(worktree, &["rebase", "--abort"])
        .map_err(|source| Error::Git {
            op: "rebase-forward abort",
            source,
        })?;
    let conflicted_ref = format!("{CONFLICTED_REF_PREFIX}{}", replay.mark_id);
    git.run(
        worktree,
        &["update-ref", conflicted_ref.as_str(), replay.mark_at],
    )
    .map_err(|source| Error::Git {
        op: "rebase-forward decline update-ref",
        source,
    })?;
    Ok(Replayed::Conflicted(paths))
}
