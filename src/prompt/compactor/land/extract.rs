//! The **extract** (`docs/DESIGN_CONTEXT_ECONOMY.md` §5.3, ARCH §2.7) —
//! the one compaction product no model authors.
//!
//! At the landing, code derives `summary/<NNN>.refs.md` from **what the
//! compaction removes from context**: the transcript entries present at
//! the compaction point and absent from the base — the `messages/` paths
//! the pass nominated for deletion ([`super::base::Product`]). It lands in
//! the base beside the compactor's own `summary/<NNN>.md`, and the name
//! sorts after it (`003.md` < `003.refs.md`), so the model reads the prose
//! first and the list second and the assembler's `drop_oldest_summaries`
//! sheds the pair together (§5.2).
//!
//! **It widens no toolset.** The compactor still has its deletion-only
//! pair; this is written by the landing exactly as the landing already
//! writes the base commit — a pure function of git, replayable, correct by
//! construction where a model's prose is correct by judgement
//! (`docs/PRINCIPLES.md` *Compaction, never compression*). Nor is it a
//! second representation of a stored fact: the span it reads is leaving
//! context, and the extract is that fact's only remaining home *in*
//! context — the entries themselves stay whole on the compactor's ref,
//! the soft archive `search_history` reads (§2.7).
//!
//! **Five sections, one fill order, one cap.** Verbatim user messages,
//! error strings, pull-request numbers, commit shas, paths ([`scan`]) —
//! each deduplicated, newest entry first, filled in that order until
//! `compaction.intermediate.extract_bytes` is spent (§6). One cap with a
//! fill order is one policy; per-section caps would be five. Bytes, not
//! tokens: the extract is a file in the tree, so the bound is stated in
//! the unit the tree has. The preamble and the truncation marker are
//! **structure, never cappable** — the same rule the tool-output envelope
//! states (§3.3) — so `extract_bytes` bounds the references and nothing
//! else, and an extract in which not one reference fits is not written at
//! all (the general path with empty inputs, not a special case).

mod scan;

use super::super::Error;
use super::span::Span;
use crate::prompt::dispatch::MESSAGES_DIR;
use crate::template::GitRunner;
use std::path::Path;

/// The extract's name beside its summary: `003.md` gains `003.refs.md`.
const REFS_SUFFIX: &str = ".refs.md";

/// The file's opening frame — structure, outside the cap (module docs).
const PREAMBLE: &str = "# references from the compacted span\n\n\
     Derived by the landing from the transcript entries this compaction removed\n\
     from context (ARCH §2.7). The entries themselves stay whole on the\n\
     compactor's ref.\n";

/// The extract as the base commit stages it: a branch-relative path and
/// the text at it.
pub(super) struct Extract {
    pub(super) path: String,
    pub(super) text: String,
}

/// Derive this landing's extract, or `None` when there is nothing to
/// write: no `extract_bytes` (severable — omit the key and no extract
/// exists), no summary for it to sit beside, or no reference in what the
/// compaction removes.
///
/// The entries are read out of the **compaction point**, never the
/// worktree: the live branch has kept stepping past it, and the point's
/// tree is the context the base is cut from.
pub(super) fn of(
    parent_worktree: &Path,
    span: &Span,
    deletions: &[String],
    summaries: &[String],
    extract_bytes: Option<usize>,
    git: &dyn GitRunner,
) -> Result<Option<Extract>, Error> {
    let (Some(cap), Some(summary)) = (extract_bytes, summaries.iter().max()) else {
        return Ok(None);
    };
    let prefix = format!("{MESSAGES_DIR}/");
    let mut removed: Vec<&String> = deletions
        .iter()
        .filter(|p| p.starts_with(&prefix))
        .collect();
    // Newest first: the transcript counter is zero-padded, so descending
    // path order is descending age order (§2.3).
    removed.sort_by(|a, b| b.cmp(a));
    let mut refs = Vec::new();
    for path in removed {
        let spec = format!("{}:{path}", span.point);
        let content = git
            .run_capture(parent_worktree, &["show", &spec])
            .map_err(|source| Error::Git {
                op: "compaction land extract read",
                source,
            })?;
        refs.push(scan::of(path, &content));
    }
    let text = render(&refs, cap);
    Ok((!text.is_empty()).then(|| Extract {
        path: format!(
            "{}{REFS_SUFFIX}",
            summary.strip_suffix(".md").unwrap_or(summary)
        ),
        text,
    }))
}

/// The extract's text for the entries' references (newest first) under a
/// `cap` on the references themselves (module docs). Empty when not one
/// reference fits — a file saying only that everything was omitted tells
/// the model nothing it can act on.
fn render(refs: &[scan::Refs], cap: usize) -> String {
    let sections = sections(refs);
    let total: usize = sections.iter().map(|s| s.1.len()).sum();
    let mut out = String::new();
    let mut emitted = 0usize;
    for (title, items) in &sections {
        let mut head = format!("\n## {title}\n\n");
        for item in items {
            let unit = format!("{head}{item}");
            if out.len() + unit.len() > cap {
                if emitted == 0 {
                    return String::new();
                }
                let omitted = total - emitted;
                return format!(
                    "{PREAMBLE}{out}\n[... extract truncated at the {cap}-byte cap: \
                     {omitted} of {total} references omitted; the removed entries stay \
                     whole on the compactor's ref ...]\n"
                );
            }
            out.push_str(&unit);
            head.clear();
            emitted += 1;
        }
    }
    if emitted == 0 {
        return String::new();
    }
    format!("{PREAMBLE}{out}")
}

/// The five sections in fill order, each deduplicated and newest first,
/// with the empty ones dropped (module docs). Items are pre-rendered, so
/// deduplication and the cap both read one string.
fn sections(refs: &[scan::Refs]) -> Vec<(&'static str, Vec<String>)> {
    let mut out = [
        ("verbatim user messages", Vec::new()),
        ("error strings", Vec::new()),
        ("pull requests", Vec::new()),
        ("commit shas", Vec::new()),
        ("paths", Vec::new()),
    ];
    for entry in refs {
        if let Some(body) = &entry.user_message {
            push(&mut out[0].1, format!("{body}\n\n"));
        }
        for (i, items) in [&entry.errors, &entry.prs, &entry.shas, &entry.paths]
            .into_iter()
            .enumerate()
        {
            for item in items {
                push(&mut out[i + 1].1, format!("- {item}\n"));
            }
        }
    }
    out.into_iter()
        .filter(|(_, items)| !items.is_empty())
        .collect()
}

/// Append `item` unless an older sighting already holds its place —
/// deduplicated, newest first.
fn push(items: &mut Vec<String>, item: String) {
    if !items.contains(&item) {
        items.push(item);
    }
}

#[cfg(test)]
mod tests;
