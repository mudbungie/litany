//! Settling a tool window nothing will finish (§2.9 stop, §6 crash).
//!
//! A tool window that ends without answers — the §2.9 stop cascade
//! felled it, or its executor died outright — leaves the step's
//! model-output entry committed (§2.5 — the assistant entry lands
//! before any tool runs) while some of its `tool_use` blocks have no
//! committed `tool_result`. Left that way the branch tip is unpaired:
//! `litany advance` declines it loudly (`Error::UnpairedToolUse`), so
//! no deposit could ever revive the agent — a stop would retire the
//! branch instead of ending the work it had in flight (contradicting
//! §2.9 "a stop is not a locked door … a message into the stopped
//! agent's inbox starts a driver and resumes the same branch"), and a
//! crash would strand it until a human forks from history.
//!
//! So the window is **settled**: one in-band `is_error` `tool_result`
//! per unanswered `tool_use` id — the same shape a grant decline and a
//! control refusal already commit ([`super::refusal`],
//! [`super::seam::refusal_text`]) — saying why there is no output. The
//! tail is then settled, the warrant is `ModelCallDue`, an ordinary
//! deposit revives the agent, and the model reads *in band* what
//! happened, which is both the truthful record and the useful one. Two
//! settlements, two sentences: [`interrupted`] is the stopped exit
//! settling its own window on the way out (§2.9, bl-b98d);
//! [`crashed`] is the next drive settling a window whose executor died
//! without one (§6, bl-4187 — reached from
//! [`crate::prompt::dispatch::advance`], before delivery, so the
//! settlement lands ahead of any mail and composes wire-legal under
//! §2.3's positional pairing).
//!
//! Deleting the tail — what the dispatch commit does at a **fork**
//! ([`super::super::step_commit::unsettled`], §2.3 step 2) — is the
//! wrong repair in either case: that tail belongs to the agent's *own*
//! branch, where discarding it would throw away the assistant's
//! reasoning and leave the model with no evidence it was ever cut off.
//!
//! A **hold** is deliberately not settled (§3.3 *Tool control*): a
//! parked branch's unpaired tail is its state, and its mark asserts
//! nothing at or past the held block ran. Both entries here run only
//! where no live mark governs the window: the stopped exit makes any
//! mark stale by §3.3's own rule, and the drive boundary adjudicates
//! the mark before it ever reaches the crash settlement.

use super::super::transcript;
use super::ToolWindow;
use crate::prompt::{Deps, Error};
use brazen::Content;
use std::path::Path;

/// Commit an interrupted `tool_result` for every `tool_use` in
/// `assistant_content` still unanswered, and report the window stopped
/// (§2.9, bl-b98d).
pub(super) fn interrupted(
    worktree: &Path,
    conv_id: &str,
    assistant_content: &[Content],
    deps: &Deps<'_>,
) -> Result<ToolWindow, Error> {
    settle(worktree, conv_id, assistant_content, stop_text, deps)?;
    Ok(ToolWindow::Stopped)
}

/// Commit a died-executor `tool_result` for every `tool_use` in
/// `assistant_content` still unanswered (§6, bl-4187): the drive
/// boundary found a markless unpaired window — its executor's lease
/// was kernel-released mid-window, the one way such a window exists.
pub(in crate::prompt::dispatch) fn crashed(
    worktree: &Path,
    conv_id: &str,
    assistant_content: &[Content],
    deps: &Deps<'_>,
) -> Result<(), Error> {
    settle(worktree, conv_id, assistant_content, crash_text, deps)
}

/// The shared settlement: one in-band `is_error` `tool_result` per
/// unanswered `tool_use` id, worded by `text`.
///
/// Idempotent by construction: the answered ids are read from the
/// transcript (the record, never a stored cursor — PRINCIPLES single
/// source of truth), so results committed before the window ended —
/// and settlements a prior entry already committed — keep the one
/// entry they already have.
fn settle(
    worktree: &Path,
    conv_id: &str,
    assistant_content: &[Content],
    text: fn(&str) -> String,
    deps: &Deps<'_>,
) -> Result<(), Error> {
    let committed = transcript::committed_result_ids(worktree)?;
    for block in assistant_content {
        let Content::ToolUse { id, name, .. } = block else {
            continue;
        };
        if committed.contains(id) {
            continue;
        }
        let tool_result = Content::ToolResult {
            tool_use_id: id.clone(),
            content: vec![Content::Text(text(name))],
            is_error: true,
        };
        transcript::commit_tool(worktree, conv_id, &tool_result, deps.git)?;
    }
    Ok(())
}

/// The in-band text an unanswered invocation carries as its `is_error`
/// `tool_result` — why there is no output, in the terms §2.9 gives it.
/// No result envelope and no exit code: nothing returned, so none is
/// invented (§3.3, the [`super::seam::refusal_text`] discipline).
fn stop_text(tool: &str) -> String {
    format!(
        "{tool:?} did not return: this agent was stopped while the invocation was in \
         flight (ARCH §2.9), so it was cut short and produced no result. Any side \
         effects it had already performed stand."
    )
}

/// The crash settlement's sentence (§6): unlike a stop, a died executor
/// recorded nothing, so whether the invocation ran at all is unknown —
/// said plainly, and the re-issue judgement left to the reader, the
/// only party with context to make it.
fn crash_text(tool: &str) -> String {
    format!(
        "{tool:?} did not return: the executor driving this window died before \
         recording an outcome (ARCH §6), so whether the invocation ran is unknown. \
         Any side effects it may have performed stand; re-issue the call if the \
         work is still wanted."
    )
}
