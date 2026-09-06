//! The **reply debt** (ARCH §2.6 *A reply is owed once*): whether this
//! branch's terminal response answers anything at all.
//!
//! A reply is an answer, and an answer is owed to a question. Nothing
//! made that a *bound*, so two agents that spoke to each other never
//! stopped: each one's terminal response was deposited into the other's
//! inbox, woke it, and produced a terminal response deposited back. One
//! `message` call in 333 transcript rows; 13.4M tokens on work that
//! finished in four steps, and neither participant could stop it from
//! inside — both said so in the responses that kept being delivered
//! (bl-82d8).
//!
//! The brake is an invariant, not a counter: **a prompt is answered
//! once.** What makes it derivable — never stored — is that the branch's
//! own transcript already records both halves of the exchange in one
//! sequence (§2.3): a delivered message is `messages/NNN-<sender>.md`,
//! and this branch's own model output is `messages/NNN-<model>.json`,
//! whose entry is a **terminal response** exactly when it declares no
//! `tool_use` (§2.5 — that is what ends a step loop). So the window this
//! terminal event answers is *everything delivered since the previous
//! terminal response*, and the debt is whether anything in that window
//! asked this agent for something:
//!
//! - a **prompt** — a delivered message from somebody else — does;
//! - an **own child's return** does: its work-product transfer lands on
//!   this branch (§2.6), so it is this agent's own commissioned work
//!   arriving toward the answer it is still composing. Read by the same
//!   predicate the drain and the interpreter read, `own_result_ref`;
//! - a **foreign reply** — a result message from an agent this one did
//!   not dispatch, a sibling's answer above all — does **not**. It is
//!   the answer to a question this agent asked, and being answered asks
//!   nothing. This one line is the loop's floor;
//! - a **self-note** (§2.11) does not: its answer is the agent's own
//!   next step, which has already happened.
//!
//! **A branch that has never answered always owes**, which is the same
//! rule with an empty window rather than a bootstrap case: with no
//! previous terminal response there is nothing this reply could be a
//! repetition of. That also covers a transcript compaction squashed the
//! record out of (§2.6) — erring toward answering, since the cost of a
//! spurious reply is one deposit and the cost of a swallowed one is a
//! conversation that never hears back.

use super::super::child_result::own_result_ref;
use super::super::transcript::MESSAGES_DIR;
use crate::prompt::Error;
use brazen::Content;
use std::path::Path;

/// The one reserved `.json` origin token (§2.3): a tool call's result,
/// never model output, so never a terminal response.
const TOOL_ORIGIN: &str = "tool";

/// One entry under `messages/`, named by the branch's transcript counter
/// (§2.3 — order lives in the filename and nowhere else). **The one
/// reading of the transcript directory** for the whole addressing rule:
/// the debt below and [`super::last_prompter`] walk the same list, so
/// `messages/` is enumerated once per terminal event and neither can
/// parse a name the other would not.
pub(super) struct Entry {
    pub(super) seq: u32,
    pub(super) origin: String,
    /// A delivered message (`.md`), as opposed to this branch's own
    /// committed step output (`.json`).
    pub(super) delivered: bool,
    pub(super) path: std::path::PathBuf,
}

/// Whether this terminal event's reply is owed to anybody (module docs),
/// over the branch's `entries` newest-first. `false` is a structural
/// no-op at the deposit *and* at the wake-up: the terminal response still
/// stands in this agent's own conversation, which is where a reply to the
/// operator is read too (§2.6).
pub(super) fn owes_a_reply(entries: &[Entry], agent_id: &str) -> Result<bool, Error> {
    let Some(previous) = previous_terminal_seq(entries)? else {
        return Ok(true);
    };
    for entry in entries.iter().take_while(|e| e.seq > previous) {
        if entry.delivered && asks_something(entry, agent_id)? {
            return Ok(true);
        }
    }
    Ok(false)
}

/// The sequence of the terminal response *before* this event's own — the
/// lower bound of the window this reply answers. `None` when the branch
/// has no earlier one (module docs: it has never answered).
///
/// This event's own terminal entry is already committed when the
/// addressing rule is asked (§2.5 — the assistant entry lands before the
/// deposit, and again before [`super::super::terminal::conclude`]
/// re-derives), so it is the newest terminal response and is dropped.
/// Reading stops at the second one found: the window is one run long.
fn previous_terminal_seq(descending: &[Entry]) -> Result<Option<u32>, Error> {
    let mut seen_own = false;
    for entry in descending {
        if entry.delivered || entry.origin == TOOL_ORIGIN || !is_terminal_response(&entry.path)? {
            continue;
        }
        if seen_own {
            return Ok(Some(entry.seq));
        }
        seen_own = true;
    }
    Ok(None)
}

/// A committed model-output entry is a **terminal response** exactly when
/// it declares no `tool_use` block — the §2.5 condition that ends a step
/// loop, read off the entry rather than restated anywhere.
fn is_terminal_response(path: &Path) -> Result<bool, Error> {
    let bytes = std::fs::read(path).map_err(Error::Io)?;
    Ok(!super::super::entry::blocks(&bytes)
        .iter()
        .any(|b| matches!(b, Content::ToolUse { .. })))
}

/// Whether a delivered entry asks this agent for something (module docs):
/// a prompt from somebody else, or an own child's return. A foreign
/// reply and a self-note ask nothing.
fn asks_something(entry: &Entry, agent_id: &str) -> Result<bool, Error> {
    if entry.origin == agent_id {
        return Ok(false);
    }
    let body = std::fs::read_to_string(&entry.path).map_err(Error::Io)?;
    let is_result = super::super::transfer::terminal_ref_of(&body).is_some();
    Ok(!is_result || own_result_ref(agent_id, &entry.origin, &body).is_some())
}

/// Every legible entry under `messages/`, **newest first** — order lives
/// in the filename (§2.3), and both readers want it descending. An absent
/// directory is an empty transcript (the general path with empty inputs),
/// and a name that is neither `NNN-<origin>.md` nor `NNN-<origin>.json`
/// is not an entry.
pub(super) fn read_entries(worktree: &Path) -> Result<Vec<Entry>, Error> {
    let dir = worktree.join(MESSAGES_DIR);
    let rd = match std::fs::read_dir(&dir) {
        Ok(rd) => rd,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(Error::Io(e)),
    };
    let mut out = Vec::new();
    for entry in rd {
        let entry = entry.map_err(Error::Io)?;
        let name = entry.file_name();
        if let Some(parsed) = name.to_str().and_then(|n| parse(n, entry.path())) {
            out.push(parsed);
        }
    }
    out.sort_unstable_by_key(|e| std::cmp::Reverse(e.seq));
    Ok(out)
}

/// Split `NNN-<origin>.<md|json>`. The origin token carries hyphens of
/// its own (an agent id's descent, §2.3), so the split is at the *first*
/// hyphen and the remainder is the whole token.
fn parse(name: &str, path: std::path::PathBuf) -> Option<Entry> {
    let (stem, delivered) = match name.strip_suffix(".md") {
        Some(stem) => (stem, true),
        None => (name.strip_suffix(".json")?, false),
    };
    let (seq, origin) = stem.split_once('-')?;
    let seq = seq.parse().ok()?;
    (!origin.is_empty()).then(|| Entry {
        seq,
        origin: origin.to_string(),
        delivered,
        path,
    })
}

#[cfg(test)]
mod tests;
