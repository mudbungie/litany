//! Assemble the model-facing wire history from the branch's worktree
//! (ARCH §2.3, §5).
//!
//! Context assembly has exactly one input: the read-state commit's tree
//! (§5.1), materialized as the branch's worktree. [`assemble`] composes
//! the §5.5 parts that ride the message array, in order:
//!
//! 1. **Head and body** ([`body`], §5.2): the manifest role's `pinned`
//!    extras and `order` categories, budgeted, as path-framed user-side
//!    text blocks. (The pinned files with structural wire homes —
//!    `goal.md`, `name` and `soul.md` in the system slot, §2.3; tool
//!    schemas and
//!    the skill descriptions their tools claim in the tools array,
//!    §3.3 — compose through those homes, not here; the standalone
//!    skill descriptions no tool claims compose as head blocks.)
//! 2. **Transcript tail** (§2.3): `messages/` sorted by the filename's
//!    `NNN` prefix — order lives in the name, no git-log walk and no
//!    index — each entry composed by its origin token:
//!    - `NNN-<sender>.md` — a delivered message (§2.11): user-role text.
//!    - `NNN-<model-id>.json` — one step's model output: the canonical
//!      [`Content`] blocks verbatim, as an assistant-role message. The
//!      origin token names the model that authored the entry (§2.3,
//!      §4.3); any token but the reserved `tool` composes
//!      assistant-side.
//!    - `NNN-tool.json` — one tool call's `tool_result` block(s):
//!      user-role content in the following wire message.
//!
//! Consecutive same-side entries group into one alternating wire
//! message, so every `tool_use` block is matched by a `tool_result` in
//! the immediately following user message *by construction* (§2.3, §2.5
//! pairing). Running, retry, and replay all call this one function
//! against one input — a commit's tree — so "replay" is not a mode
//! (§2.3 *Crash and recovery*). [`transcript`] composes part 2 alone —
//! the §6 warrant derivation reads the transcript tail and must not see
//! body material (and must stay config-free for lazy resolution).

mod body;

use super::entry;
use crate::config::manifest::RoleRules;
use crate::prompt::Error;
use brazen::{Content, Message, Role};
use std::path::{Path, PathBuf};

/// Branch-scoped transcript directory (ARCH §2.3 — `messages/NNN-…`).
const MESSAGES_DIR: &str = "messages";
/// The one reserved `.json` origin token (§2.3): a `tool` entry composes
/// tool-side as `tool_result` content — canonical [`Role::Tool`], which
/// each brazen protocol projects into its own dialect (the anthropic
/// projection folds it into a `"user"` message; ollama/openai fan it out
/// to `role:"tool"` messages). Every other `.json` token is a model id
/// and composes assistant-side; every `.md` sender composes user-side.
const TOOL_ORIGIN: &str = "tool";

/// Which wire side an entry composes onto (§2.3). Grouping is by side,
/// not by [`Role`], so the enum can derive `PartialEq` without leaning
/// on brazen's type.
#[derive(PartialEq, Eq, Clone, Copy)]
enum Side {
    User,
    Assistant,
    Tool,
}

impl Side {
    fn role(self) -> Role {
        match self {
            Side::User => Role::User,
            Side::Assistant => Role::Assistant,
            Side::Tool => Role::Tool,
        }
    }
}

/// Assemble the full §5.2/§5.5 wire message history: the manifest
/// role's head-and-body blocks ([`body`]), then the transcript tail.
/// `rules` is the role's manifest entry from the governing config
/// commit (§2.2); a role the manifest does not list assembles
/// transcript-only — the general path with empty inputs, not a special
/// case.
pub(in crate::prompt) fn assemble(
    worktree: &Path,
    rules: Option<&RoleRules>,
) -> Result<Vec<Message>, Error> {
    let mut messages: Vec<Message> = Vec::new();
    for text in body::compose(worktree, rules)? {
        push_grouped(&mut messages, Side::User, vec![Content::Text(text)]);
    }
    append_transcript(&mut messages, worktree)?;
    Ok(messages)
}

/// Assemble the transcript tail alone (§2.3): the §6 warrant derivation
/// reads only the tail's wire side, before any config is resolved, so
/// head/body material must not lead the history it inspects.
pub(super) fn transcript(worktree: &Path) -> Result<Vec<Message>, Error> {
    let mut messages: Vec<Message> = Vec::new();
    append_transcript(&mut messages, worktree)?;
    Ok(messages)
}

/// Append the wire messages of the transcript under
/// `<worktree>/messages/` (ARCH §2.3, §5). An absent or empty directory
/// appends nothing — the general path with empty inputs, not a
/// bootstrap special case.
fn append_transcript(messages: &mut Vec<Message>, worktree: &Path) -> Result<(), Error> {
    let dir = worktree.join(MESSAGES_DIR);
    let mut entries: Vec<(u32, PathBuf)> = match std::fs::read_dir(&dir) {
        Ok(rd) => {
            let mut v = Vec::new();
            for entry in rd {
                let path = entry.map_err(Error::Io)?.path();
                if let Some(seq) = seq_of(&path) {
                    v.push((seq, path));
                }
            }
            v
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Vec::new(),
        Err(e) => return Err(Error::Io(e)),
    };
    entries.sort_by_key(|(seq, _)| *seq);

    for (_, path) in entries {
        let (side, content) = compose_entry(&path)?;
        push_grouped(messages, side, content);
    }
    Ok(())
}

/// The `NNN` counter of a `messages/NNN-<origin>.<ext>` path (the prefix
/// before the first `-`). A non-conforming name contributes no entry.
fn seq_of(path: &Path) -> Option<u32> {
    path.file_name()?
        .to_string_lossy()
        .split('-')
        .next()
        .and_then(|p| p.parse::<u32>().ok())
}

/// Compose one transcript entry into its wire `(side, content)`
/// (ARCH §2.3 *Origins and wire framing*). Role framing is derived from
/// the path — the extension and origin token — never from frontmatter.
fn compose_entry(path: &Path) -> Result<(Side, Vec<Content>), Error> {
    if path.extension().and_then(|e| e.to_str()) == Some("md") {
        let body = std::fs::read_to_string(path).map_err(Error::Io)?;
        return Ok((Side::User, vec![Content::Text(body)]));
    }
    let bytes = std::fs::read(path).map_err(Error::Io)?;
    // The entry shape has one home (§2.3, `super::entry`): a bare block
    // array or the `content` object, both harness-written, so the read
    // cannot fail — and a model entry's `usage` sibling is telemetry for
    // transcript readers, no part of the wire message.
    Ok((entry_side(path), entry::blocks(&bytes)))
}

/// A `.json` entry composes tool-side (a `tool_result`) iff its origin
/// token is the reserved `tool` (§2.3); every other token is a model id
/// (the entry's author) and composes assistant-side as model output. The
/// origin token is the stem past its `NNN-` counter prefix — model ids
/// carry hyphens (`claude-fable-5`), so the split keeps everything after
/// the first.
fn entry_side(path: &Path) -> Side {
    let stem = path
        .file_stem()
        .map(|s| s.to_string_lossy())
        .unwrap_or_default();
    let origin = stem.split_once('-').map(|x| x.1).unwrap_or_default();
    if origin == TOOL_ORIGIN {
        Side::Tool
    } else {
        Side::Assistant
    }
}

/// Append `content` to the trailing message when it is the same side,
/// else start a new message (ARCH §2.3 — consecutive same-side entries
/// group into one alternating wire message).
fn push_grouped(messages: &mut Vec<Message>, side: Side, mut content: Vec<Content>) {
    match messages.last_mut() {
        Some(last) if last.role == side.role() => last.content.append(&mut content),
        _ => messages.push(Message {
            role: side.role(),
            content,
        }),
    }
}

#[cfg(test)]
mod tests;
