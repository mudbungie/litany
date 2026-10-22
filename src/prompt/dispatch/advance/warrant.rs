//! What the transcript tail **warrants** (ARCH §6 hop step 3): the one
//! derivation that decides whether a hop steps, exits silently, or
//! declines an unpaired window. Pure over the assembled wire history —
//! no disk, no config — which is what lets a no-op hop stay config-free
//! (§6 lazy resolution). Unit tests: [`super::tests`].

use brazen::{Content, Message, Role};

/// What the transcript tail warrants (§6 hop step 3).
#[derive(Debug, PartialEq, Eq)]
pub(super) enum Warrant {
    /// Tail ends user-side (delivered mail, committed tool results): a
    /// model call is due.
    ModelCallDue,
    /// Tail ends assistant-side without `tool_use`, or is empty: nothing
    /// is due (§2.11 pin 1).
    NothingDue,
    /// A `tool_use` unmatched by any committed `tool_result` — wherever
    /// it sits: the one non-replayable state (§6), declined loudly.
    Unpaired,
}

/// Derive warrant from the assembled wire history (§6). The §2.3
/// pairing invariant is judged over the **whole alternation**, not the
/// tail: a `tool_use` with no committed `tool_result` is [`Warrant::Unpaired`]
/// even with delivered mail behind it — the tail's role cannot answer
/// for it, because `driver::deliver` runs before this derivation, so a
/// crash-orphaned window is routinely *buried* user-side by the time it
/// is read (bl-15f0; sent anyway, the provider rejects the history
/// forever). Only a fully paired history lets the tail speak: user- or
/// tool-side means a model call is due (committed tool results compose
/// tool-side, canonical `Role::Tool`, §2.3, and delivered mail
/// user-side — the same observation), assistant-side or empty means
/// nothing is due.
pub(super) fn warrant(messages: &[Message]) -> Warrant {
    let mut unanswered = std::collections::HashSet::new();
    for m in messages {
        for b in &m.content {
            match b {
                Content::ToolUse { id, .. } => {
                    unanswered.insert(id.as_str());
                }
                Content::ToolResult { tool_use_id, .. } => {
                    unanswered.remove(tool_use_id.as_str());
                }
                _ => {}
            }
        }
    }
    if !unanswered.is_empty() {
        return Warrant::Unpaired;
    }
    match messages.last() {
        Some(m) if matches!(m.role, Role::User | Role::Tool) => Warrant::ModelCallDue,
        _ => Warrant::NothingDue,
    }
}
