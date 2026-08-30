//! The held-branch entry of a `litany advance` hop (ARCH §3.3 *Tool
//! control*): resume a tool window the configured control parked.
//!
//! A hold mark ([`crate::workspace::hold`]) is checked **before
//! delivery**: a parked branch's tail is an assistant entry with
//! unresolved `tool_use`, and delivering mail onto it would wedge a
//! user entry between a `tool_use` and its eventual `tool_result` —
//! breaking the §2.3 pairing the next request must ship. So a held hop
//! delivers nothing (mail queues; every pending deposit is accounted as
//! seen, [`drain::seen_all`], so the §2.11 release rule relaunches
//! nothing for a branch that stays parked) and instead re-enters the
//! tool window at the frontier: committed results are skipped
//! ([`tool_step::run_tool_calls`]'s transcript-derived skip) and the
//! once-held invocation is **re-adjudicated freshly** — the control may
//! now pass, refuse, or hold again. Release is therefore not a harness
//! verb: whatever out-of-band fact changes the control's answer is the
//! control's contract, and the harness guarantees exactly one thing —
//! re-adjudication on the next drive of the agent.
//!
//! A mark whose invocation is no longer open (its result committed, or
//! the tail past the window) is **stale** — a crash between a resumed
//! window's completion and its bookkeeping — and is cleared, the hop
//! continuing as if it never existed ([`Resumption::Stale`]).

use super::super::{assembler, child_result, drain, driver, terminal, tool_step};
use super::AdvanceOutcome;
use crate::config::Event;
use crate::prompt::inbox::{self, Epitaph, ExecutorLock};
use crate::prompt::resolve::WorkerConfig;
use crate::prompt::step::{next_step_seq, step_dir_rel};
use crate::prompt::workflow_actions;
use crate::prompt::{Deps, Error};
use crate::workspace::hold;
use brazen::{Content, Message, Role};
use std::path::Path;

/// What the held entry decided.
pub(super) enum Resumption {
    /// The hop is fully handled (resumed to a handoff, parked again, or
    /// stepped to a terminal).
    Done(AdvanceOutcome),
    /// The mark was stale and has been cleared: the caller continues the
    /// ordinary hop with the lease handed back.
    Stale(ExecutorLock),
}

/// Run the held entry for `agent_id` under `mark`.
pub(super) fn resume(
    workspace: &Path,
    agent_id: &str,
    mark: &hold::Held,
    lock: ExecutorLock,
    deps: &Deps<'_>,
    resolve: &mut dyn FnMut() -> Result<WorkerConfig, Error>,
) -> Result<Resumption, Error> {
    let seen = drain::seen_all(&inbox::inbox_dir(workspace, agent_id))?;
    let worktree = crate::workspace::agent_worktree(workspace, agent_id);
    if !worktree.exists() {
        // A parked branch keeps its worktree (only quiescent and
        // terminal exits tear down, and a hold is neither); kept total
        // rather than declared unreachable — the branch stays parked.
        driver::release_then_reprobe(lock, workspace, agent_id, &seen, deps.launcher);
        return Ok(Resumption::Done(AdvanceOutcome::NothingToDo));
    }
    let tail = assembler::transcript(&worktree)?;
    let Some(window) = open_window(&tail).filter(|w| w.unpaired(&mark.tool_use_id)) else {
        hold::clear(workspace, agent_id, deps.git).map_err(|source| Error::Git {
            op: "stale hold mark clear",
            source,
        })?;
        return Ok(Resumption::Stale(lock));
    };
    let cfg = resolve()?;
    let resolved = cfg.as_resolved();
    // The emitting step is the last one on record — its `tools/` subtree
    // is where the resumed calls' records belong (§3.3 Disk record).
    let step_seq = next_step_seq(workspace, agent_id)?.saturating_sub(1).max(1);
    let step_dir_rel_str = step_dir_rel(agent_id, step_seq);
    match tool_step::run_tool_calls(
        workspace,
        &worktree,
        agent_id,
        &resolved,
        &step_dir_rel_str,
        window.content,
        deps,
    )? {
        tool_step::ToolWindow::Held => {
            // Parked again (the mark restated by the seam): release and
            // exit, exactly as the first park did.
            driver::release_then_reprobe(lock, workspace, agent_id, &seen, deps.launcher);
            Ok(Resumption::Done(AdvanceOutcome::Held))
        }
        tool_step::ToolWindow::Stopped => {
            terminal::conclude(
                workspace,
                agent_id,
                Epitaph::Stopped,
                &cfg.workflow,
                lock,
                &seen,
                deps,
            )?;
            Ok(Resumption::Done(AdvanceOutcome::Terminal))
        }
        tool_step::ToolWindow::Completed => {
            // The window is whole again: the same post-window seams the
            // hop runs (§6 — on_tool_return, then the compaction clock),
            // then the ordinary exec-baton handoff; the successor
            // delivers the queued mail at its own step boundary.
            workflow_actions::run_step_hook(
                &cfg.workflow,
                Event::OnToolReturn,
                &worktree,
                agent_id,
                deps.git,
            )?;
            child_result::run_flush(workspace, agent_id, &worktree, &cfg.workflow, deps)?;
            Ok(Resumption::Done(AdvanceOutcome::ToolsPending(lock)))
        }
    }
}

/// The transcript tail's open tool window: the last assistant message's
/// content, plus the result ids committed after it.
struct OpenWindow<'a> {
    content: &'a [Content],
    resolved_ids: Vec<&'a str>,
}

impl OpenWindow<'_> {
    /// Is `id` a `tool_use` of this window with no committed result?
    fn unpaired(&self, id: &str) -> bool {
        self.content
            .iter()
            .any(|b| matches!(b, Content::ToolUse { id: block_id, .. } if block_id == id))
            && !self.resolved_ids.contains(&id)
    }
}

/// The last assistant message and the `tool_result` ids committed after
/// it, or `None` for a tail with no assistant entry. (A partially
/// resumed window's tail ends tool-side, so this looks *back* to the
/// last assistant message rather than only at the tail message.)
fn open_window(tail: &[Message]) -> Option<OpenWindow<'_>> {
    let at = tail.iter().rposition(|m| m.role == Role::Assistant)?;
    let resolved_ids = tail[at + 1..]
        .iter()
        .flat_map(|m| m.content.iter())
        .filter_map(|b| match b {
            Content::ToolResult { tool_use_id, .. } => Some(tool_use_id.as_str()),
            _ => None,
        })
        .collect();
    Some(OpenWindow {
        content: &tail[at].content,
        resolved_ids,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn msg(role: Role, content: Vec<Content>) -> Message {
        Message { role, content }
    }

    /// `open_window` is total over arbitrary tails: a text block after
    /// the last assistant message contributes no resolved id, a tail
    /// with no assistant entry has no window, and pairing is judged on
    /// the ids alone.
    #[test]
    fn open_window_is_total_over_arbitrary_tails() {
        assert!(open_window(&[msg(Role::User, vec![Content::Text("hi".into())])]).is_none());
        let tail = [
            msg(
                Role::Assistant,
                vec![
                    Content::ToolUse {
                        id: "t1".into(),
                        name: "bash".into(),
                        input: serde_json::json!({}),
                        signature: None,
                    },
                    Content::ToolUse {
                        id: "t2".into(),
                        name: "bash".into(),
                        input: serde_json::json!({}),
                        signature: None,
                    },
                ],
            ),
            msg(
                Role::Tool,
                vec![Content::ToolResult {
                    tool_use_id: "t1".into(),
                    content: vec![],
                    is_error: false,
                }],
            ),
            msg(Role::User, vec![Content::Text("mail".into())]),
        ];
        let window = open_window(&tail).unwrap();
        assert!(!window.unpaired("t1"), "t1 has a committed result");
        assert!(window.unpaired("t2"), "t2 is the open frontier");
        assert!(!window.unpaired("t9"), "t9 is not of this window");
    }
}
