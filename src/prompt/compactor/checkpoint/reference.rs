//! **Where this branch's clock measures from** — the reference commit
//! and the commit subjects that mark it (ARCH §2.6, §2.7).
//!
//! Split out of [`super`] at the seam its own docs already name: that
//! module is the predicate and the state it reads, this one is the
//! git-log question every consumer of "since the last checkpoint" asks —
//! the clock ([`super::state`]) from `HEAD`, and the compaction landing
//! ([`super::super::land`]) from the compaction point, where the same
//! commit is the span's lower bound. One derivation, two readers.

use super::Error;
use crate::prompt::role;
use crate::template::GitRunner;
use std::path::Path;

/// Subject prefix of a **compaction base** commit ([`super::super::land`]) — the
/// single commit a landing squashes the compaction span into (ARCH §2.6).
/// The most recent such commit marks the last checkpoint; commits after it
/// are what a fresh `every_n_commits`/`every_t_seconds` trigger measures
/// from — exactly the branch's uncompacted content, since everything the
/// landing replayed on top of the base is what the span left out.
pub(in crate::prompt::compactor) const BASE_SUBJECT_PREFIX: &str = "compaction base [";

/// Subject prefix of a retired compaction-*merge* commit. The merge-back
/// landing is replaced by rebase-forward (ARCH §2.6, bl-bc9c), but
/// histories that predate the replacement still carry these commits, and
/// the clock must keep reading them as checkpoints.
pub(in crate::prompt::compactor) const MERGE_SUBJECT_PREFIX: &str = "compaction merge [";

/// The sha the branch's checkpoint clock measures from: the newest commit
/// reachable from `start` that is **this branch's own founding commit**
/// (its dispatch commit, matched by [`role::founding_pattern`] — the one
/// home of that question), a **compaction base**
/// ([`BASE_SUBJECT_PREFIX`]), or a retired **compaction merge**
/// ([`MERGE_SUBJECT_PREFIX`]). `git log -n1` walks newest-first and stops
/// at the first match, and multiple `--grep` patterns are OR'd, so one
/// query answers "where does this branch's own clock start". The clock reads it from `HEAD` ([`state`]); the landing
/// reads it from the compaction point, where it is the **span's lower
/// bound** — the parent of the base commit it mints ([`super::super::land`]).
///
/// `None` — no such commit reachable — falls back to the branch root
/// ([`checkpoint_time`], [`commits_since`]). That is the general path with
/// empty inputs, not a bootstrap special case: a tree with no dispatch
/// commit at all has nothing else to measure from.
pub(in crate::prompt::compactor) fn origin(
    worktree: &Path,
    start: &str,
    agent_id: &str,
    git: &dyn GitRunner,
) -> Result<Option<String>, Error> {
    let founding = role::founding_pattern(agent_id);
    let based = format!("^{}", regex_escape_brackets(BASE_SUBJECT_PREFIX));
    let merged = format!("^{}", regex_escape_brackets(MERGE_SUBJECT_PREFIX));
    let out = git
        .run_capture(
            worktree,
            &[
                "log",
                "-n",
                "1",
                "--format=%H",
                "-E",
                "--grep",
                founding.as_str(),
                "--grep",
                based.as_str(),
                "--grep",
                merged.as_str(),
                start,
            ],
        )
        .map_err(|source| Error::Git {
            op: "checkpoint log grep",
            source,
        })?;
    let sha = out.trim();
    Ok((!sha.is_empty()).then(|| sha.to_string()))
}

/// The root commit reachable from `rev` (its eldest parentless ancestor) —
/// the base-parent fallback when [`self::origin`] finds nothing, exposed for the
/// landing ([`super::super::land`]) so both consumers share one derivation.
pub(in crate::prompt::compactor) fn root_of(
    worktree: &Path,
    rev: &str,
    git: &dyn GitRunner,
) -> Result<String, Error> {
    let out = git
        .run_capture(worktree, &["rev-list", "--max-parents=0", rev])
        .map_err(|source| Error::Git {
            op: "checkpoint root rev-list",
            source,
        })?;
    Ok(out.lines().last().unwrap_or("").trim().to_string())
}

/// Escape the one regex metacharacter a commit-subject *prefix* constant
/// can carry (`[`), so a literal prefix reads as a literal under `git log
/// -E`. Keeping both `--grep` patterns in one regex dialect is what lets
/// the two questions [`self::origin`] asks collapse into one git call.
fn regex_escape_brackets(literal: &str) -> String {
    literal.replace('[', r"\[")
}

/// One anchored `-E` pattern matching either landing subject — a
/// compaction base or a retired-mechanism merge — built from the same
/// constants [`self::origin`] greps, so the span's overtaken check
/// ([`super::super::land`]) and the clock cannot drift apart.
pub(in crate::prompt::compactor) fn landing_subject_pattern() -> String {
    format!(
        "^({}|{})",
        regex_escape_brackets(BASE_SUBJECT_PREFIX),
        regex_escape_brackets(MERGE_SUBJECT_PREFIX)
    )
}
