//! The **compaction product** (ARCH §2.6, §2.7): what one pass takes
//! out of the dispatching branch's context and what it leaves in its
//! place — classified entirely from git, never from a sidecar.
//!
//! Three things reach the base commit and nothing else does.
//!
//! - **Nominations.** A path *deleted* in `dispatch..tip` on the
//!   compactor's branch is a `mark_for_deletion` call (the fork-time
//!   prunes — the empty-grant `descriptions/**` derivation, the
//!   unsettled-tool-step removal — all land *on* the dispatch commit and
//!   are thereby excluded, structurally). This is where the compactor's
//!   judgement lives: work products, and the earlier summary this pass
//!   supersedes.
//! - **The summary.** A path *added under `summary/`* in the same range
//!   is the `write_summary` product.
//! - **The span's transcript entries**, swept by the landing itself
//!   ([`span_transcript`], bl-2071,
//!   `docs/DESIGN_CONTEXT_ECONOMY.md` §5.5). Which entries a summary
//!   replaces is not a judgement — it is the span, and the span is
//!   derived from git ([`super::span`]). So a pass that wrote a summary
//!   takes **every** `messages/**` path in the tree at the compaction
//!   point out of the base, save two: the branch's dispatch entry
//!   (`eligibility::is_dispatch_entry`, §2.7's first not-eligible class)
//!   and a trailing **unsettled tool window**, which is one unit with
//!   the results replaying on top of it ([`span_transcript`], bl-2d93).
//!   A retained tail is already outside the sweep: `keep_recent` /
//!   `keep_recent_tokens` move the compaction point back, and only the
//!   point's tree is read here.
//!
//! Nothing else exists to a landing: the compactor's dialog, goal, and
//! soul are additions outside `summary/` and rewrites, which this module
//! never reads.
//!
//! **Why the sweep is the landing's and not the soul's** (bl-2071).
//! Measured: four landed compactions over 94 transcript commits
//! reclaimed *nothing*. The shipped soul told the model to supersede its
//! predecessor's summary and never named the entries the summary stood
//! for, so compaction became pure addition — a model dispatch per
//! checkpoint carrying the whole inherited transcript, and a prompt that
//! grew from 8k to 91k tokens across the four. Prose cannot carry an
//! invariant a landing can derive.

use super::super::Error;
use super::super::tools::eligibility;
use super::extract::{self, Extract};
use super::span::Span;
use crate::prompt::dispatch::{entry, pairing};
use crate::template::GitRunner;
use brazen::Content;
use std::path::Path;

/// The compaction product: what the compactor's two tools committed after
/// its dispatch commit (module docs), plus the one product no model
/// authors — the **extract** the landing itself derives (docs/TAXONOMY.md
/// §3, [`extract`]) — and nothing else.
pub(super) struct Product {
    /// What leaves the base's tree: the paths nominated by
    /// `mark_for_deletion` (deleted in `dispatch..tip` on the
    /// compactor's branch), plus — when the pass wrote a summary — the
    /// span's transcript entries the landing sweeps itself (module docs).
    /// One list because the two have one effect and one reader: the
    /// `git rm --cached` in [`super::base`], and the extract that reports
    /// what left.
    pub(super) deletions: Vec<String>,
    /// `summary/**` paths added by `write_summary`.
    pub(super) summaries: Vec<String>,
    /// `summary/<NNN>.refs.md`, derived here from what the deletions take
    /// out of context; `None` when the workflow declares no
    /// `extract_bytes`, when no summary was written for it to sit beside,
    /// or when nothing referable was removed ([`extract::of`]).
    pub(super) extract: Option<Extract>,
}

impl Product {
    /// No deletions and no summary: nothing to land ([`super::LandOutcome::NoOp`]).
    pub(super) fn is_empty(&self) -> bool {
        self.deletions.is_empty() && self.summaries.is_empty()
    }
}

/// Classify the compaction product from the compactor's branch (module
/// docs): deletions and `summary/**` additions in `dispatch..tip`, then
/// the extract derived from the first of those two.
pub(super) fn product(
    parent_worktree: &Path,
    span: &Span,
    compactor_ref: &str,
    extract_bytes: Option<usize>,
    git: &dyn GitRunner,
) -> Result<Product, Error> {
    let dispatch = span.dispatch.as_str();
    let mut deletions = diff_class(parent_worktree, dispatch, compactor_ref, "D", None, git)?;
    let summaries = diff_class(
        parent_worktree,
        dispatch,
        compactor_ref,
        "A",
        Some("summary"),
        git,
    )?;
    // The sweep is conditional on a summary and on nothing else (module
    // docs): the summary is what stands in for the span, so a pass that
    // wrote none has nothing to stand in for the entries it would take.
    if !summaries.is_empty() {
        for entry in span_transcript(parent_worktree, span, git)? {
            if !deletions.contains(&entry) {
                deletions.push(entry);
            }
        }
    }
    let extract = extract::of(
        parent_worktree,
        span,
        &deletions,
        &summaries,
        extract_bytes,
        git,
    )?;
    Ok(Product {
        deletions,
        summaries,
        extract,
    })
}

/// Paths of one `--diff-filter` class between two trees, optionally
/// limited to a pathspec. `--no-renames` keeps the classes exhaustive: an
/// add/delete pair must not collapse into an `R` that escapes both.
fn diff_class(
    parent_worktree: &Path,
    from: &str,
    to: &str,
    class: &str,
    pathspec: Option<&str>,
    git: &dyn GitRunner,
) -> Result<Vec<String>, Error> {
    let filter = format!("--diff-filter={class}");
    let mut args = vec![
        "diff",
        "--name-only",
        "--no-renames",
        filter.as_str(),
        from,
        to,
    ];
    if let Some(spec) = pathspec {
        args.extend(["--", spec]);
    }
    let out = git
        .run_capture(parent_worktree, &args)
        .map_err(|source| Error::Git {
            op: "compaction land product diff",
            source,
        })?;
    Ok(out
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .map(str::to_owned)
        .collect())
}

/// The span's transcript entries — every `messages/**` path in the tree
/// at the compaction point except the branch's dispatch entry (module
/// docs) and except a trailing **unsettled tool window** (below). Read
/// from the point, never the worktree: the live branch has kept
/// stepping past it, and the point's tree is what the base is cut from.
///
/// `ls-tree` rather than a diff against the span's lower bound: what the
/// summary replaces is everything in context at the point, and after the
/// previous landing those are the same set. Stating it as the tree's
/// contents rather than as a range makes the base's invariant one a test
/// can read off the base alone — its `messages/` holds the dispatch
/// entry and nothing else.
///
/// **The sweep stops at the last settled boundary** (bl-2d93). A
/// compaction point is an arbitrary commit — `HEAD~keep_recent` counts
/// commits, and the token tail lands on a model entry's own commit
/// (§5.2) — while a **tool window** spans several: the model output
/// entry commits, then each `tool_result` entry commits after its tool
/// returns (§2.5). So a point routinely falls *inside* a window, with
/// the call in the point's tree and its results in the live tail. Swept
/// flat, that takes the call out of the base and leaves the results
/// replaying on top of it — an orphan `tool_result`, which every
/// provider refuses (`pairing`: Anthropic as "tool_result without
/// tool_use", the OpenAI Responses API as "No tool call found for
/// function call output with call_id …") on *every* later prompt, so
/// the branch is wedged rather than merely mis-compacted. A tool call
/// and its result are one unit for every cut, so the trailing unsettled
/// window stays in the base: the same tail the fork prune deletes from
/// the compactor's own tree ([`crate::prompt::dispatch::step_commit`]),
/// which is also exactly what the compactor never read and never
/// summarized.
fn span_transcript(
    parent_worktree: &Path,
    span: &Span,
    git: &dyn GitRunner,
) -> Result<Vec<String>, Error> {
    let out = git
        .run_capture(
            parent_worktree,
            &[
                "ls-tree",
                "-r",
                "--name-only",
                "-z",
                &span.point,
                "--",
                crate::prompt::dispatch::MESSAGES_DIR,
            ],
        )
        .map_err(|source| Error::Git {
            op: "compaction land span transcript",
            source,
        })?;
    let mut entries = pairing::ordered(
        out.split_terminator('\0')
            .filter(|p| !p.is_empty())
            .map(str::to_owned)
            .collect(),
    );
    if let Some(cut) = pairing::unsettled_from(&entries, &|rel: &str| {
        blob(parent_worktree, &span.point, rel, git)
    })? {
        entries.truncate(cut);
    }
    entries.retain(|p| !eligibility::is_dispatch_entry(p));
    Ok(entries)
}

/// The canonical blocks of one transcript entry as of `point` — the
/// blob, never the worktree, for the same reason the listing is the
/// point's tree: the live branch has moved on. Read through the entry
/// shape's one home (§2.3, [`entry`]).
fn blob(
    parent_worktree: &Path,
    point: &str,
    rel: &str,
    git: &dyn GitRunner,
) -> Result<Vec<Content>, Error> {
    let spec = format!("{point}:{rel}");
    let out = git
        .run_capture(parent_worktree, &["show", &spec])
        .map_err(|source| Error::Git {
            op: "compaction land span entry read",
            source,
        })?;
    Ok(entry::blocks(out.as_bytes()))
}
