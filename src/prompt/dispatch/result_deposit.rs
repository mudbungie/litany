//! Result-message deposit at a terminal event (ARCH §2.6, §2.3 step 5).
//!
//! Every terminal event of a step loop deposits a **result message** —
//! "Return is not a verb" (`docs/PRINCIPLES.md`): the deposit is
//! executor-side, never a model `message` tool call. This module is the
//! executor's side of that return: it derives the recipient
//! ([`recipient`]), reads the branch tip as the terminal ref (§2.6), and
//! deposits with the matching epitaph.
//!
//! **Who the result is addressed to is decided by the epitaph's value**
//! (§2.6 — code branches on the value, never on the message's shape):
//! a **reply** (`final-response`) answers whoever last prompted this
//! agent; an **obituary** (`stopped`, `budget-exhausted`, `died`) reports
//! to the dispatcher, whose address is the agent's own id minus its last
//! descent segment ([`inbox::parent_of`], §2.11). Both can be absent —
//! a reply to the user and a root's obituary alike deposit nothing — and
//! the absent arm is one structural no-op, not two special cases (§2.4:
//! the terminal response answers the user, who reads this agent's own
//! conversation).
//!
//! **The deposit does not launch.** Waking the recipient this deposit
//! revives (§2.11 revival-on-deposit) is the exit protocol's closing
//! act, not the deposit's return value: it happens once, after the
//! depositing executor releases its own lock, and by epitaph value —
//! [`super::terminal::exit_launch`] / `revive_recipient`, which addresses
//! the same [`recipient`] the deposit did. Keeping it there keeps this a
//! pure return and keeps the launch decision in one place.

use super::super::inbox::{self, Epitaph, USER_SENDER};
use super::super::{Deps, Error};
use super::step_commit::read_branch_tip;
use super::transfer::terminal_ref_of;
use brazen::Content;
use std::path::Path;

mod debt;

/// The terminal response body iff the agent spoke: the concatenated
/// [`Content::Text`] blocks of the final assistant content, or `None`
/// when it produced none (§2.6 — the body is present exactly when the
/// agent spoke). Thinking blocks are not speech and are excluded.
pub(super) fn terminal_text(blocks: &[Content]) -> Option<String> {
    let mut out = String::new();
    for block in blocks {
        if let Content::Text(text) = block {
            out.push_str(text);
        }
    }
    (!out.is_empty()).then_some(out)
}

/// The inbox this terminal event's result message is addressed to (§2.6
/// *A reply answers the last prompter; an obituary reports to the
/// dispatcher*), or `None` when no agent is addressed.
///
/// An **obituary** — every epitaph but `final-response` — is a
/// structural fact about the tree rather than an answer to anyone: it
/// says *this agent is gone*, and the one party that has a standing
/// interest in that is the agent that dispatched it. Its address is the
/// id's ([`inbox::parent_of`]), which no rewrite of the transcript can
/// move, so the dispatcher hears about a stop, an exhausted ceiling or a
/// death even when the branch was mid-conversation with somebody else.
///
/// A **reply** — `final-response` — answers whoever last prompted this
/// agent ([`last_prompter`]), **if it owes one at all** ([`debt`]). For
/// the dispatch step the last prompter *is* the dispatcher (the goal
/// arrives as its message, §2.5), so the old parent-addressed rule is
/// this rule's first case rather than a rule of its own; `user` is
/// nobody's inbox, so an operator-prompted reply deposits nothing and is
/// read in this agent's own conversation.
///
/// **A reply is owed once, to a question** (§2.6, bl-82d8). Nothing
/// bounded the answer before, so two agents that spoke to each other
/// answered each other's answers forever. The debt is derived from this
/// branch's own transcript — has anything been delivered since the
/// previous terminal response that *asks* this agent for something —
/// and a terminal event that owes nothing addresses nobody, which is the
/// same structural no-op an operator-prompted reply already takes.
pub(super) fn recipient(
    worktree: &Path,
    agent_id: &str,
    epitaph: Epitaph,
) -> Result<Option<String>, Error> {
    if epitaph != Epitaph::FinalResponse {
        return Ok(inbox::parent_of(agent_id));
    }
    // One reading of `messages/` for both halves of the rule (§2.3 —
    // order lives in the filename, so the list comes back newest first).
    let entries = debt::read_entries(worktree)?;
    if !debt::owes_a_reply(&entries, agent_id)? {
        return Ok(None);
    }
    Ok(match last_prompter(&entries, agent_id)? {
        Some(sender) if sender == USER_SENDER => None,
        Some(sender) => Some(sender),
        // No surviving prompt: the dispatch message is the transcript's
        // first, so its absence means compaction squashed the record
        // (§2.6) — and the id still carries the one sender the branch's
        // own existence records. A root has neither, and deposits
        // nothing.
        None => inbox::parent_of(agent_id),
    })
}

/// The **last prompter**: the sender of the newest delivered message in
/// this branch's transcript that is a prompt from somebody else (§2.3 —
/// order lives in the filename, so the newest is max-`NNN`; the origin
/// token names the sender). Derived, never stored: a stored "who spoke
/// last" would be a second copy of what `messages/` already says
/// (`docs/PRINCIPLES.md` Single source of truth).
///
/// Two entries are skipped, each because it is not a prompt:
///
/// - **A returning child's result message** — a delivered entry carrying
///   `terminal_ref:` frontmatter (§2.6). It is the answer to a dispatch
///   this agent already made, not a question put to it; without the skip
///   every parent would address its own answer to the last child that
///   returned.
/// - **This agent's own note to itself** (§2.11 *Self-messages*). Its
///   answer is the agent's own next step, which has already happened;
///   addressing a reply to one's own inbox would deposit into the very
///   inbox whose delivery produced it and never terminate.
fn last_prompter(entries: &[debt::Entry], agent_id: &str) -> Result<Option<String>, Error> {
    for entry in entries.iter().filter(|e| e.delivered) {
        if entry.origin != agent_id
            && terminal_ref_of(&std::fs::read_to_string(&entry.path)?).is_none()
        {
            return Ok(Some(entry.origin.clone()));
        }
    }
    Ok(None)
}

/// Deposit this branch's result message on its own behalf at a terminal
/// event (§2.3 step 5): derive the recipient ([`recipient`]), read the
/// branch tip as the terminal ref, then deposit with `epitaph` and the
/// `response` body. Addressed to nobody — an operator-prompted reply, a
/// root's obituary — it is one structural no-op; wired at the call site
/// so a step loop returns without new plumbing.
pub(super) fn deposit_terminal(
    repo: &Path,
    conv_id: &str,
    worktree: &Path,
    epitaph: Epitaph,
    response: Option<&str>,
    deps: &Deps<'_>,
) -> Result<(), Error> {
    let Some(recipient) = recipient(worktree, conv_id, epitaph)? else {
        return Ok(());
    };
    let terminal_ref = read_branch_tip(worktree, deps)?;
    inbox::deposit_result(
        repo,
        &recipient,
        conv_id,
        epitaph,
        &terminal_ref,
        response,
        deps.clock,
        deps.git,
    )?;
    Ok(())
}

#[cfg(test)]
mod tests;
