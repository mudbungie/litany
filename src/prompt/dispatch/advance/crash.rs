//! §6 crash settlement (bl-4187): the drive boundary settles a
//! markless unpaired trailing window before anything else touches the
//! branch.
//!
//! A tool window whose executor died — SIGKILL, OOM, power loss, a
//! panic past the assistant commit — leaves `tool_use` blocks with no
//! committed `tool_result` and no hold mark. Under the mark-first
//! ordering in [`super::run`], the lease the hop holds over a markless
//! unpaired window was necessarily kernel-released mid-window: a live
//! window's executor holds its lease for the window's whole life, and a
//! parked window carries the §3.3 mark that is adjudicated before this
//! runs. So what this settles is always a corpse, never a rival.
//!
//! The settlement is [`super::tool_step::settle::crashed`] — the
//! bl-b98d shape with the crash sentence — and it runs **before
//! delivery** on purpose: §2.3's pairing is positional, so a
//! settlement appended after delivered mail could never compose
//! wire-legal. Settled first, the mail lands behind a paired window,
//! the warrant reads `ModelCallDue`, and the deposit revives the
//! branch. A window already buried user-side (pre-settlement debris)
//! cannot be repaired by appending and stays the §6 loud decline.

use super::super::{assembler, tool_step};
use crate::prompt::{Deps, Error};
use brazen::{Content, Role};
use std::path::Path;

/// Settle the trailing window if it is a markless crash orphan; every
/// other shape — no worktree (a torn-down branch is settled by
/// construction, §2.3 step 6), no assistant entry, a settled window,
/// buried debris — is left exactly as found. The transcript is read
/// once here, ahead of the post-delivery read the warrant makes — the
/// same whole-directory scan assembly already pays, once extra per hop.
pub(super) fn settle_crashed_window(
    workspace: &Path,
    agent_id: &str,
    deps: &Deps<'_>,
) -> Result<(), Error> {
    let worktree = crate::workspace::agent_worktree(workspace, agent_id);
    if !worktree.exists() {
        return Ok(());
    }
    let messages = assembler::transcript(&worktree)?;
    let Some(window) = messages.iter().rposition(|m| m.role == Role::Assistant) else {
        return Ok(());
    };
    if messages[window + 1..].iter().any(|m| m.role == Role::User) {
        return Ok(());
    }
    let mut unanswered: std::collections::HashSet<&str> = messages[window]
        .content
        .iter()
        .filter_map(|b| match b {
            Content::ToolUse { id, .. } => Some(id.as_str()),
            _ => None,
        })
        .collect();
    for m in &messages[window + 1..] {
        for b in &m.content {
            if let Content::ToolResult { tool_use_id, .. } = b {
                unanswered.remove(tool_use_id.as_str());
            }
        }
    }
    if unanswered.is_empty() {
        return Ok(());
    }
    eprintln!(
        "litany: settling a crashed tool window on [{agent_id}] — {} unanswered \
         invocation(s) recorded as died (ARCH §6, bl-4187)",
        unanswered.len()
    );
    tool_step::settle::crashed(&worktree, agent_id, &messages[window].content, deps)
}
