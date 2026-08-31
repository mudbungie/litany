//! §5.2 head-and-body composition: the manifest-governed,
//! non-transcript part of assembled context.
//!
//! The manifest role's `pinned` list selects the **head extras** —
//! always included regardless of budget — and its `order` list fills
//! the **body** in declared category order, lexically within each
//! category (§5.5 "category-sorted body"), until the token budget's
//! overflow policy kicks in. Each selected worktree file composes as
//! one path-framed text block (§5.3 file path as hint).
//!
//! Material whose wire home is structural never composes here (§5.1
//! consequences, §2.3, §3.3): `goal.md`, `name` and `soul.md` ride the
//! system slot ("Goal and soul are pinned files, not sequence item
//! zero", §2.3 — the name as the identity line
//! [`crate::prompt::dispatch::step_commit::compose_system`] derives),
//! `descriptions/tools/**` — and every skill description a tool
//! claims ([`tool_backed`]) — rides the tools array (§3.3 tools-list
//! assembly), and `messages/**` is the transcript tail, always last
//! (§5.2). The rest of `descriptions/**` is Description-always'
//! standalone-skill remainder (§3.3): no structural home, so it composes
//! here as ordinary path-framed blocks — the general path, not a case.
//!
//! **The budget covers head + body, not the transcript.** §5.2 subjects
//! only `order` entries to `budget_tokens` (pinned counts but is never
//! shed); the transcript is deliberately outside it, so the body stays
//! a pure function of inputs that change only at rebuild points (§5.5
//! "Between rebuild points the body is stable") — a growing transcript
//! never evicts a summary mid-branch. The transcript's pressure valve
//! is compaction (§2.7, §6), not assembly.

use crate::config::manifest::{OverflowPolicy, RoleRules};
use crate::prompt::Error;
use std::path::Path;

/// Token estimate: ~4 bytes/token (the English-text heuristic,
/// `docs/TAXONOMY.md` "Token and tokenizer"). The budget is an estimate
/// by construction — litany carries no provider tokenizer (§4.2 keeps
/// provider facts out of the harness); framing overhead is not counted.
const BYTES_PER_TOKEN: u64 = 4;

/// Transcript home (§2.3): never head or body material.
const TRANSCRIPT_DIR: &str = "messages";
/// Committed tool schemas (§3.3): their wire home is the tools array.
const TOOLS_DESC_DIR: &str = "descriptions/tools";
/// Committed skill frontmatter (§3.3 Description-always), one
/// `<name>.md` per available skill.
const SKILLS_DESC_PREFIX: &str = "descriptions/skills/";
/// The category `drop_oldest_summaries` sheds from (§2.7 —
/// `summary/NNN.md`, zero-padded, so lexical order is age order).
const SUMMARY_PREFIX: &str = "summary/";

/// One selected worktree file.
struct Entry {
    /// Worktree-relative path (§5.3 — the hint the framing preserves).
    path: String,
    content: String,
}

impl Entry {
    fn tokens(&self) -> u64 {
        (self.content.len() as u64).div_ceil(BYTES_PER_TOKEN)
    }
}

/// Compose the role's head extras and budgeted body as rendered text
/// blocks, in assembly order. `None` (a role the manifest does not
/// list) composes nothing — the general path with empty inputs.
pub(super) fn compose(worktree: &Path, rules: Option<&RoleRules>) -> Result<Vec<String>, Error> {
    let Some(rules) = rules else {
        return Ok(Vec::new());
    };
    let files = walk(worktree)?;
    let mut taken = vec![false; files.len()];
    let head = select(worktree, &rules.pinned, &files, &mut taken)?;
    let body = select(worktree, &rules.order, &files, &mut taken)?;
    // Pinned is always included and counts toward the budget (§5.2
    // "regardless of budget"); what remains is the body's allowance.
    let spent: u64 = head.iter().map(Entry::tokens).sum();
    let allowance = u64::from(rules.budget_tokens).saturating_sub(spent);
    let body = fit(body, allowance, rules.overflow);
    Ok(head.into_iter().chain(body).map(render).collect())
}

/// §5.3 file path as hint: the worktree-relative path rides the
/// assembled block as its frame.
fn render(e: Entry) -> String {
    format!("<file path=\"{}\">\n{}\n</file>", e.path, e.content)
}

/// Expand `patterns` (in declared order — category order is priority)
/// against the walked file list, reading each selected file once.
/// `taken` spans invocations so a file matched by `pinned` never re-enters
/// via `order`. Non-UTF-8 bytes compose lossily: assembled context is
/// text, and declining a skill's stray binary asset would hold the
/// whole branch hostage to it.
fn select(
    worktree: &Path,
    patterns: &[String],
    files: &[String],
    taken: &mut [bool],
) -> Result<Vec<Entry>, Error> {
    let mut out = Vec::new();
    for pattern in patterns {
        for (i, file) in files.iter().enumerate() {
            if taken[i] || !glob_match(pattern, file) {
                continue;
            }
            taken[i] = true;
            let bytes = std::fs::read(worktree.join(file)).map_err(Error::Io)?;
            let content = String::from_utf8_lossy(&bytes).into_owned();
            if marked_summary(file, &content) {
                return Err(Error::SummaryConflictMarkers { path: file.clone() });
            }
            out.push(Entry {
                path: file.clone(),
                content,
            });
        }
    }
    Ok(out)
}

/// §5.2 marker guard, the read-path half of the §2.6 marker-freedom
/// promise: `summary/**`'s only sanctioned writer is `write_summary`,
/// and the compaction landing declines any content conflict during
/// the replay ([`crate::prompt::compactor::land`]), so a summary
/// carrying a git conflict-marker line is a violated invariant however
/// it arrived — a pre-guard tree, an operator hand-edit, a payload
/// quoting the literal strings — and composing it would send corrupted
/// context as if it were the branch's history (§2.7). Matched are the
/// two labelled marker lines git writes (`<<<<<<< <label>`,
/// `>>>>>>> <label>`); a bare `=======` never trips the guard — git
/// never writes one without the flanking labelled pair, and it is a
/// legitimate setext heading underline in model-authored markdown.
/// Other categories are authored content under no such promise and
/// compose unguarded: refusing a skill asset that documents merge
/// conflicts would hold the branch hostage to a stray asset, the same
/// argument that keeps non-UTF-8 composition lossy rather than fatal
/// ([`select`]).
fn marked_summary(path: &str, content: &str) -> bool {
    path.starts_with(SUMMARY_PREFIX)
        && content
            .lines()
            .any(|l| l.starts_with("<<<<<<< ") || l.starts_with(">>>>>>> "))
}

/// Apply the role's overflow policy to a body whose estimate exceeds
/// its allowance (§5.2 "order entries fill the body in declared order
/// until overflow policy kicks in"). A fitting body passes untouched.
///
/// Every policy in the vocabulary is an act on the tree in hand, over
/// material that tree can hold — so every arm below sheds. Model-driven
/// shedding was never such an act: assembly is a pure function of the
/// tree (§5.1) and cannot invoke a model, so it belongs to the §6
/// compaction checkpoint, declared once in `workflow.yaml`
/// `compaction:` (§5.2 — the retired `summarize` policy). Neither was
/// shedding step records, which live outside every worktree (§2.2,
/// §2.3) — hence no no-op arm here, and no `drop_oldest_steps` to reach
/// it (§5.2, bl-7846).
fn fit(body: Vec<Entry>, allowance: u64, policy: OverflowPolicy) -> Vec<Entry> {
    let total: u64 = body.iter().map(Entry::tokens).sum();
    if total <= allowance {
        return body;
    }
    match policy {
        OverflowPolicy::DropOldestSummaries => drop_oldest_summaries(body, allowance),
        OverflowPolicy::Truncate => cut(body, allowance, true),
        OverflowPolicy::Drop => cut(body, allowance, false),
    }
}

/// Shed lexically-first `summary/**` entries — oldest first (§2.7
/// zero-padded names) — until the body fits or none remain; a residual
/// overflow with no summaries left rides.
fn drop_oldest_summaries(mut body: Vec<Entry>, allowance: u64) -> Vec<Entry> {
    let mut total: u64 = body.iter().map(Entry::tokens).sum();
    while total > allowance {
        let oldest = body
            .iter()
            .enumerate()
            .filter(|(_, e)| e.path.starts_with(SUMMARY_PREFIX))
            .min_by(|a, b| a.1.path.cmp(&b.1.path))
            .map(|(i, _)| i);
        let Some(i) = oldest else {
            break;
        };
        total -= body[i].tokens();
        body.remove(i);
    }
    body
}

/// Fill in declared order until the first entry that does not fit; the
/// overflowing entry is truncated to the remaining allowance
/// (`truncate: true`) or dropped whole (`false`), and later entries
/// never fill past the kick-in point.
fn cut(body: Vec<Entry>, allowance: u64, truncate: bool) -> Vec<Entry> {
    let mut kept = Vec::new();
    let mut remaining = allowance;
    for mut e in body {
        let t = e.tokens();
        if t <= remaining {
            remaining -= t;
            kept.push(e);
            continue;
        }
        if truncate && remaining > 0 {
            // t > remaining guarantees the byte cut lands short of the
            // content's end; back off to a char boundary.
            let mut n = usize::try_from(remaining * BYTES_PER_TOKEN).expect("fits: n < len");
            while !e.content.is_char_boundary(n) {
                n -= 1;
            }
            e.content.truncate(n);
            kept.push(e);
        }
        break;
    }
    kept
}

/// Every file in the worktree, as sorted worktree-relative paths —
/// minus the structurally-homed trees ([`skip`]). An absent worktree
/// walks empty. Lexical sort is the §5.5 category sort.
fn walk(worktree: &Path) -> Result<Vec<String>, Error> {
    let mut out = Vec::new();
    descend(worktree, worktree, "", &mut out)?;
    out.sort();
    Ok(out)
}

fn descend(worktree: &Path, dir: &Path, prefix: &str, out: &mut Vec<String>) -> Result<(), Error> {
    let rd = match std::fs::read_dir(dir) {
        Ok(rd) => rd,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(e) => return Err(Error::Io(e)),
    };
    for entry in rd {
        let entry = entry.map_err(Error::Io)?;
        let name = entry.file_name().to_string_lossy().into_owned();
        let rel = if prefix.is_empty() {
            name
        } else {
            format!("{prefix}/{name}")
        };
        if skip(worktree, &rel) {
            continue;
        }
        if entry.file_type().map_err(Error::Io)?.is_dir() {
            descend(worktree, &entry.path(), &rel, out)?;
        } else {
            out.push(rel);
        }
    }
    Ok(())
}

/// What context assembly composes through a structural home rather than
/// body text (module doc): the transcript tail, the system slot, the
/// tools array — plus `.git`, which is git's, not the tree's.
fn skip(worktree: &Path, rel: &str) -> bool {
    rel == ".git"
        || rel == TRANSCRIPT_DIR
        || rel == TOOLS_DESC_DIR
        || crate::prompt::dispatch::step_commit::SYSTEM_SLOT_FILES.contains(&rel)
        || tool_backed(worktree, rel)
}

/// Whether `rel` is a skill description a tool claims: a
/// `descriptions/skills/<name>.md` with a `descriptions/tools/<name>.json`
/// beside it. Its frontmatter `description` is that tool's `tools:` entry
/// description (§3.3 point 3) — its wire home — so composing it as text
/// too would send it twice. Everything else under `descriptions/skills/`
/// is the standalone remainder (§3.3), homed in the head. The tree is
/// the sole input, as §5.1 requires: the role's `tools:` list selects
/// among tool-backed skills; a skill no tool claims has no such selector.
fn tool_backed(worktree: &Path, rel: &str) -> bool {
    let stem = rel
        .strip_prefix(SKILLS_DESC_PREFIX)
        .and_then(|f| f.strip_suffix(".md"));
    let Some(name) = stem else { return false };
    let schema = format!("{name}.json");
    worktree.join(TOOLS_DESC_DIR).join(schema).exists()
}

mod glob;
use glob::glob_match;

#[cfg(test)]
mod tests;
