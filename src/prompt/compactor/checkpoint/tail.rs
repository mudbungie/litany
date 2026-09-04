//! **The token tail** — the compaction point derived from successive
//! usage reports rather than a commit count
//! (`docs/DESIGN_CONTEXT_ECONOMY.md` §5.2, `compaction.intermediate.
//! keep_recent_tokens`).
//!
//! `keep_recent: k` says "keep the last `k` commits", which is a proxy:
//! commits differ by orders of magnitude in what they cost to re-send.
//! `keep_recent_tokens: n` states the thing itself — **keep the longest
//! stretch of the transcript that costs at most `n` prompt tokens to
//! append** — and it costs no new state, because every model entry
//! already records what the branch's prompt side stood at when it landed
//! (ARCH §2.3 *Usage rides the entry*). The difference between two of
//! those reports is what the stretch between them costs. No tokenizer,
//! no stored counter, no estimate: subtraction over two numbers the
//! provider stated ([`super::usage`]).
//!
//! **The point is a step boundary by construction.** A prompt count
//! exists only where a model entry landed, and a model entry lands in
//! its own transcript commit, so every candidate the walk considers is
//! already a lawful fork point — nothing has to round one.
//!
//! **The walk is bounded by the checkpoint origin** ([`super::reference::
//! origin`]) — this branch's own founding commit or its last compaction
//! base, whichever is newer — the same lower bound the span uses
//! ([`super::super::land`]). A branch whose whole uncompacted stretch
//! fits in `n` has **nothing to compact**: the point would sit at the
//! origin, the span would be empty, and the flush skips (the general
//! path with empty inputs, exactly as an under-`keep_recent` span does).

use super::{Error, reference, usage};
use crate::template::GitRunner;
use std::path::{Path, PathBuf};

/// The compaction point under a token tail: `Some(sha)` when there is a
/// stretch beneath the retained tail to compact, `None` when the whole
/// uncompacted branch fits in `budget` (module docs) or has no model
/// entry to measure at all.
pub(in crate::prompt) fn point(
    worktree: &Path,
    agent_id: &str,
    budget: u32,
    git: &dyn GitRunner,
) -> Result<Option<String>, Error> {
    let Some(tip) = usage::last(worktree)? else {
        return Ok(None);
    };
    let budget = u64::from(budget);
    let mut point = None;
    for (sha, path) in entries(worktree, agent_id, git)? {
        let Some(report) = read(worktree, &sha, &path, git)? else {
            continue;
        };
        // Newest-first, so the cost of the retained stretch grows as the
        // walk goes older. The last candidate still inside the budget is
        // the point; the first one outside ends the walk, and its own
        // commit is the newest thing the span may swallow.
        if tip.prompt_tokens.saturating_sub(report.prompt_tokens) > budget {
            return Ok(point);
        }
        point = Some(sha);
    }
    // Never exceeded: the branch's whole uncompacted stretch is the tail.
    Ok(None)
}

/// `(sha, path)` of every commit since the checkpoint origin that **adds
/// a model entry**, newest first. `git log --diff-filter=A --name-only`
/// answers both halves in one call; a line under `messages/` is a path
/// and anything else non-empty is the commit sha that owns the paths
/// following it.
fn entries(
    worktree: &Path,
    agent_id: &str,
    git: &dyn GitRunner,
) -> Result<Vec<(String, PathBuf)>, Error> {
    let range = match reference::origin(worktree, "HEAD", agent_id, git)? {
        Some(sha) => format!("{sha}..HEAD"),
        None => "HEAD".to_string(),
    };
    let dir = format!("{}/", usage::MESSAGES_DIR);
    let out = git
        .run_capture(
            worktree,
            &[
                "log",
                "--format=%H",
                "--name-only",
                "--diff-filter=A",
                &range,
                "--",
                &dir,
            ],
        )
        .map_err(|source| Error::Git {
            op: "token tail log",
            source,
        })?;
    let mut found = Vec::new();
    let mut sha = String::new();
    for line in out.lines().filter(|l| !l.trim().is_empty()) {
        if let Some(rest) = line.strip_prefix(&dir) {
            let path = PathBuf::from(line);
            if usage::model_entry(Path::new(rest)).is_some() {
                found.push((sha.clone(), path));
            }
        } else {
            sha = line.trim().to_string();
        }
    }
    Ok(found)
}

/// One candidate's usage report, read out of the commit that added it —
/// the blob, never the worktree, so the walk reads what each step
/// actually recorded rather than what the tip happens to hold.
fn read(
    worktree: &Path,
    sha: &str,
    path: &Path,
    git: &dyn GitRunner,
) -> Result<Option<usage::LastUsage>, Error> {
    let name = path.to_string_lossy();
    let spec = format!("{sha}:{name}");
    let blob = git
        .run_capture(worktree, &["show", &spec])
        .map_err(|source| Error::Git {
            op: "token tail entry read",
            source,
        })?;
    Ok(usage::report(blob.as_bytes(), &name))
}

#[cfg(test)]
mod tests;
