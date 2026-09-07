//! **The pairing invariant** (ARCH §2.3, §2.5, §3.3): a tool call and
//! the result answering it are **one unit**, and no cut may split them.
//!
//! Every provider validates the pair positionally. Anthropic refuses a
//! `tool_result` whose `tool_use` is absent (*"tool_result without
//! tool_use"*); the OpenAI Responses API refuses the same shape as
//! *"No tool call found for function call output with call_id …"*.
//! Neither refusal is recoverable in band: the history is a pure
//! function of the branch's tree (§5.1), so every later prompt carries
//! the same orphan and dies the same way — the branch is wedged, and
//! only an edit to the record or to what assembly composes gets it back
//! (bl-2d93; three live conversations died this way).
//!
//! A **tool window** is one model-output entry's `tool_use` blocks plus
//! the `messages/NNN-tool.json` entries answering them (§3.3). The
//! transcript cuts the harness makes are therefore held to one rule —
//! *a cut falls between windows, never inside one* — and this module is
//! that rule's one home, read from both sides:
//!
//! - **The write side.** [`unsettled_from`] answers where a transcript's
//!   trailing unsettled window begins, over an ordered entry list and a
//!   reader for one entry's blocks — the worktree's files at a fork
//!   ([`super::step_commit::unsettled`], which deletes that tail), a
//!   commit's blobs at the compaction landing
//!   ([`crate::prompt::compactor::land`], which declines to sweep it).
//!   Only the tail can be unsettled: the executor commits every result
//!   before the next model call (§2.5 pairing), so one look at the end
//!   is total. The other two cuts are already whole-window by
//!   construction and need nothing here — a response cut at the output
//!   cap is never sealed, so no entry is committed
//!   ([`super::model_call`], `Error::OutputTruncated`), and a window a
//!   stop or a crash felled is *settled* with one in-band `is_error`
//!   result per unanswered call ([`super::tool_step::settle`]).
//! - **The read side.** [`drop_orphans`] refuses to compose an orphan
//!   `tool_result` onto the wire at all, and names what it dropped so
//!   the step record can carry it ([`super::assembler`]). The write side
//!   stops new orphans; this is what revives a branch already carrying
//!   one, at its next prompt rather than never.

use crate::prompt::Error;
use brazen::{Content, Message};
use std::collections::HashSet;
use std::path::Path;

/// What a transcript entry is, derived from its path alone (§2.3
/// *Origins and wire framing*): the extension and the reserved-token
/// test, never frontmatter.
#[derive(PartialEq, Eq)]
pub(in crate::prompt) enum Kind {
    /// `NNN-<sender>.md` — a delivered message (§2.11).
    Message,
    /// `NNN-tool.json` — one tool call's result.
    Tool,
    /// `NNN-<model-id>.json` — one step's model output.
    Model,
}

/// The one reserved `.json` origin token (§2.3): a tool call's result.
/// Every other `.json` token is the model id that authored the entry.
const TOOL_ORIGIN: &str = "tool";

/// [`Kind`] of the transcript-relative or worktree-relative `rel`.
pub(in crate::prompt) fn kind(rel: &str) -> Kind {
    let path = Path::new(rel);
    if path.extension().and_then(|e| e.to_str()) != Some("json") {
        return Kind::Message;
    }
    let stem = path
        .file_stem()
        .map(|s| s.to_string_lossy())
        .unwrap_or_default();
    match stem.split_once('-').map(|x| x.1) {
        Some(TOOL_ORIGIN) => Kind::Tool,
        _ => Kind::Model,
    }
}

/// `paths` in transcript order — by the filename's `NNN` counter, which
/// is where order lives and nowhere else (§2.3). A name carrying no
/// counter contributes no entry. Sorted by the *parsed* counter rather
/// than lexically, so a branch past `999` — where the zero pad stops
/// making the two orders agree — still reads in the order it was
/// written.
pub(in crate::prompt) fn ordered(paths: Vec<String>) -> Vec<String> {
    let mut numbered: Vec<(u32, String)> = paths
        .into_iter()
        .filter_map(|rel| Some((seq_of(&rel)?, rel)))
        .collect();
    numbered.sort_by_key(|(seq, _)| *seq);
    numbered.into_iter().map(|(_, rel)| rel).collect()
}

/// The `NNN` counter of a `messages/NNN-<origin>.<ext>` path.
fn seq_of(rel: &str) -> Option<u32> {
    Path::new(rel)
        .file_name()?
        .to_string_lossy()
        .split('-')
        .next()
        .and_then(|p| p.parse::<u32>().ok())
}

/// The index in `entries` ([`ordered`]) at which the trailing
/// **unsettled** window begins, or `None` when the tail is settled.
///
/// The window is the branch's *last* model-output entry plus everything
/// after it; it is unsettled when some `tool_use` id that entry emitted
/// has no `tool_result` naming it among the following tool entries.
/// `blocks` reads one entry's canonical blocks from whatever the caller
/// is cutting — a worktree file, a commit's blob — and is called only
/// for the entries the answer depends on: the last model entry and the
/// tool entries after it.
pub(in crate::prompt) fn unsettled_from(
    entries: &[String],
    blocks: &dyn Fn(&str) -> Result<Vec<Content>, Error>,
) -> Result<Option<usize>, Error> {
    let Some(cut) = entries.iter().rposition(|rel| kind(rel) == Kind::Model) else {
        return Ok(None);
    };
    let mut pending: Vec<String> = blocks(&entries[cut])?
        .iter()
        .filter_map(|b| match b {
            Content::ToolUse { id, .. } => Some(id.clone()),
            _ => None,
        })
        .collect();
    let answering = entries[cut + 1..]
        .iter()
        .filter(|rel| kind(rel) == Kind::Tool);
    for rel in answering {
        for block in blocks(rel)? {
            if let Content::ToolResult { tool_use_id, .. } = block {
                pending.retain(|id| *id != tool_use_id);
            }
        }
    }
    Ok((!pending.is_empty()).then_some(cut))
}

/// Drop every **orphan** `tool_result` from an assembled wire history —
/// a result whose `tool_use` no message before it carries — and answer
/// the ids dropped, in the order they were met (module docs).
///
/// A message left with no content at all goes with its last block: an
/// empty message is not a lawful wire message anywhere, and the entry it
/// came from is exactly the one that must not be sent. Its neighbours
/// then **re-group**, because removing a message from the middle can
/// leave two same-side messages adjacent and §2.3's framing is that
/// consecutive same-side entries are one wire message — the drop must
/// not hand a provider a second illegal shape in place of the first.
pub(in crate::prompt) fn drop_orphans(messages: &mut Vec<Message>) -> Vec<String> {
    let mut called: HashSet<String> = HashSet::new();
    let mut dropped: Vec<String> = Vec::new();
    for message in &mut *messages {
        for block in &message.content {
            if let Content::ToolUse { id, .. } = block {
                called.insert(id.clone());
            }
        }
        message.content.retain(|block| match block {
            Content::ToolResult { tool_use_id, .. } if !called.contains(tool_use_id) => {
                dropped.push(tool_use_id.clone());
                false
            }
            _ => true,
        });
    }
    messages.retain(|m| !m.content.is_empty());
    let mut grouped: Vec<Message> = Vec::new();
    for message in messages.drain(..) {
        match grouped.last_mut() {
            Some(last) if last.role == message.role => last.content.extend(message.content),
            _ => grouped.push(message),
        }
    }
    *messages = grouped;
    dropped
}

#[cfg(test)]
mod tests;
