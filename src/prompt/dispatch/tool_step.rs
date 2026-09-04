//! Per-step tool-call orchestration (ARCH §2.5, §3.3).
//!
//! When a step's completion carries `tool_use` blocks, the loop hands
//! each one to [`crate::prompt::ToolExecutor`] in emission order. The
//! executor lands `input.json` and `output.json` under
//! `<conv-repo>/steps/<conv-id>/<NNN>/tools/<tool-id>/` — outside every
//! worktree (§2.2 / §2.3), a diagnostic record that is *not* a commit.
//!
//! As each tool resolves, its canonical `tool_result` block *is*
//! committed — `messages/NNN-tool.json`, the transcript entry the next
//! step's request composes from (§2.3, §3.3 "Wire `tool_result` framing
//! is transcript-backed"). Nothing is returned to the loop: the next
//! step re-assembles its whole history from the tree (§5), so a
//! `tool_result` has exactly one home, the committed entry. The per-tool-call
//! `output.json` stays the raw audit capture, written but never read at
//! runtime (§2.3 Diagnostic-only contract) — two facts, not two copies.
//! The sequential loop *is* the sibling-tool serialization §3.3
//! requires, and the counter read (`next_seq`) rides inside it.
//!
//! The loop answers no tool name itself: the multi-tool, which used to
//! be the one exception, retired into the `python` built-in, whose
//! program writes the same list as it runs (ARCH §3.3 *The program*).
//! Every invocation this window clears goes to the executor.
//!
//! One tail every result carries: the **context files** on the agent's
//! working-directory path it has not been shown yet ([`context`], §3.3)
//! — discovered here because only the window holds all three facts it
//! takes, the cwd, the workflow and the transcript.
//!
//! Living in a sibling module keeps `super`'s `run_exchange` body under
//! the repo's 300-line code-file cap.

mod context;
pub(super) mod permit;
pub(super) mod seam;
pub(in crate::prompt::dispatch) mod settle;
#[cfg(test)]
mod tests;

use super::Resolved;
use super::stop_signal;
use super::transcript;
use crate::prompt::Deps;
use crate::prompt::Error;
use crate::prompt::tool::{ExecError, ToolCall as ToolUse, ToolOutcome};
use crate::workspace::hold;
use brazen::Content;
use permit::refusal;
use std::path::Path;

/// How one tool window ended (§3.3).
#[derive(Debug)]
pub(in crate::prompt) enum ToolWindow {
    /// Every block resolved and committed; the next model call is due.
    Completed,
    /// The §2.9 stop cascade landed in the window: every `tool_use`
    /// left unanswered is settled with an in-band interrupted
    /// `tool_result` ([`settle`]) — so the tail a deposit later revives
    /// is resumable, never the §6 unpaired decline — and the caller
    /// runs the clean stopped exit.
    Stopped,
    /// The configured control held an invocation before it executed
    /// (§3.3 *Tool control*): the hold mark is written, nothing after
    /// the held block ran or committed, and the caller parks the branch
    /// — no terminal, no `tool_result` — until a later drive
    /// re-adjudicates ([`super::advance`]).
    Held,
}

/// Drive every `tool_use` block in `assistant_content` through the
/// executor in emission order, committing each result as a transcript
/// entry (§2.3, §4.4 `Content::ToolResult`). The next step's request is
/// re-assembled from the tree (§5), so nothing flows back through the
/// loop.
///
/// The executor's SIGTERM flag ([`Deps::stop`], §2.9 step 3) is the stop
/// signal handed to each tool, so a `litany stop` landing in a
/// tool-execution window is the *same* terminal sequence as one landing in
/// a model-call window: the tool subprocesses are the executor's limbs and
/// take the group SIGTERM (§2.9 steps 1-2). A tool cut down that way
/// returns [`ExecError::KilledBySignal`]; with the stop flag set that is
/// the stop, not a harness fault — the window is settled ([`settle`]) and
/// this returns [`ToolWindow::Stopped`] so [`super::run_exchange`] ceases
/// the loop for the clean stopped-deposit exit, never an error propagation. A
/// `KilledBySignal` with *no* stop pending is a genuine crash (SIGSEGV, …)
/// and still surfaces as [`Error::ToolExec`] (§2.10).
/// [`ToolWindow::Completed`] means every block resolved and the loop
/// continues.
///
/// Between the grant gate and the executor sits the **tool-control
/// seam** ([`seam`], §3.3 *Tool control*): when the governing workflow
/// names a control, every invocation the grant permits is adjudicated —
/// pass enters the executor unchanged, refuse commits the control's
/// reason as an in-band `is_error` `tool_result`, and hold writes the
/// hold mark ([`hold`]) and ceases the window ([`ToolWindow::Held`])
/// with nothing executed or committed at or past the held block. The
/// grant gate runs first: a control adjudicates only what the role may
/// call at all — grants are structure, controls are policy.
///
/// `resolved` carries the calling agent's role and its `providers.yaml`
/// `tools:` grant (§4.3) — the pair travels from the one resolution that
/// reads both ([`crate::prompt::resolve`]), so a role and a grant that
/// do not belong together cannot reach here. Its workflow also supplies
/// the `tool_output:` bounded-projection policy (§3.3, §6), handed to
/// every `execute` so the executor caps the streams it renders. They gate what may be
/// *called*, which the request's declaration does not imply: a request
/// declares every tool its history names so the wire holds
/// ([`super::tools::close_over_history`]), and an inherited transcript
/// names whatever tools the dispatching branch used. A role reaching for
/// a tool outside its effective toolset ([`refusal`]) is declined in-band
/// — an `is_error` `tool_result` committed like any other, so the model
/// reads the decline and steps on — and the executor is never entered.
pub(in crate::prompt) fn run_tool_calls(
    conv_repo: &Path,
    worktree: &Path,
    conv_id: &str,
    resolved: &Resolved<'_>,
    step_dir_rel_str: &str,
    assistant_content: &[Content],
    deps: &Deps<'_>,
) -> Result<ToolWindow, Error> {
    let step_dir_abs = conv_repo.join(step_dir_rel_str);
    let control = resolved.workflow.tool_control.as_ref();
    let stopped = || settle::interrupted(worktree, conv_id, assistant_content, deps);
    // A hold mark in play means this window is a resume (§3.3 *Tool
    // control*): blocks whose results already committed are skipped —
    // derived from the transcript, never a stored cursor — and the mark
    // lifts the moment its invocation re-adjudicates to any verdict but
    // hold. The mark is read unconditionally: the mark, not the config,
    // asserts the park, and a control since removed from the workflow
    // must still see the resume skip committed results and lift the
    // mark (an absent control adjudicates every invocation as pass) —
    // re-running a committed block would double its side effects and
    // commit a second `tool_result` for one `tool_use` id, breaking the
    // §2.3 pairing. The committed-ids read stays gated on the mark, so
    // an unparked window pays only the one ref probe.
    let mut marked = hold::read(conv_repo, conv_id, deps.git);
    let committed = match marked {
        Some(_) => transcript::committed_result_ids(worktree)?,
        None => Default::default(),
    };
    for block in assistant_content {
        let Content::ToolUse {
            id, name, input, ..
        } = block
        else {
            continue;
        };
        if committed.contains(id) {
            continue;
        }
        let mut outcome = match refusal(
            resolved.grant.role,
            resolved.grant.tools,
            &super::tools::injected(resolved.grant.role, deps.tool_executor, conv_repo, conv_id),
            name,
        ) {
            Some(decline) => ToolOutcome {
                content: decline.into_bytes(),
                is_error: true,
            },
            None => {
                let gate = seam::adjudicate(
                    control,
                    resolved.grant.role,
                    id,
                    name,
                    input,
                    conv_repo,
                    conv_id,
                    deps.stop,
                )?;
                // The mark asserts "held before execution — nothing ran"
                // (`workspace::hold`), so it must lift *before* the once-
                // held invocation can enter the executor or decline; a
                // control fault (`Err` above) leaves it standing, keeping
                // the branch parked rather than stranded.
                if matches!(gate, seam::Gate::Proceed | seam::Gate::Refuse(_))
                    && marked.as_ref().is_some_and(|m| m.tool_use_id == *id)
                {
                    hold::clear(conv_repo, conv_id, deps.git).map_err(|source| Error::Git {
                        op: "hold mark clear",
                        source,
                    })?;
                    marked = None;
                }
                match gate {
                    seam::Gate::Stopped => return stopped(),
                    seam::Gate::Hold(reason) => {
                        let held = hold::Held {
                            tool_use_id: id.clone(),
                            tool: name.clone(),
                            reason,
                        };
                        hold::write(conv_repo, conv_id, &held, deps.git).map_err(|source| {
                            Error::Git {
                                op: "hold mark write",
                                source,
                            }
                        })?;
                        return Ok(ToolWindow::Held);
                    }
                    seam::Gate::Refuse(reason) => ToolOutcome {
                        content: seam::refusal_text(name, &reason).into_bytes(),
                        is_error: true,
                    },
                    seam::Gate::Proceed => match deps.tool_executor.execute(
                        ToolUse { id, name, input },
                        &step_dir_abs,
                        deps.stop,
                        resolved.workflow.tool_output,
                    ) {
                        Ok(outcome) => outcome,
                        // §2.9 step 3: a tool group-killed by the executor's
                        // own SIGTERM, with the stop flag set, is the stop —
                        // cease the loop for the stopped-deposit exit, not an
                        // error.
                        Err(ExecError::KilledBySignal { .. })
                            if stop_signal::stopped(deps.stop) =>
                        {
                            return stopped();
                        }
                        Err(source) => {
                            return Err(Error::ToolExec {
                                tool: name.clone(),
                                source,
                            });
                        }
                    },
                }
            }
        };
        // The context files the agent's cwd path carries and its
        // transcript has not shown it yet ([`context`], §3.3) ride out
        // on this result — the tail of the entry about to commit, so
        // the pinned head is untouched (§5.5).
        context::append(
            &mut outcome.content,
            conv_repo,
            worktree,
            conv_id,
            resolved.workflow,
            deps.git,
        )?;
        let tool_result = outcome_to_tool_result(id, &outcome);
        transcript::commit_tool(worktree, conv_id, &tool_result, deps.git)?;
    }
    Ok(ToolWindow::Completed)
}

/// Turn the executor's [`ToolOutcome`] into the canonical `ToolResult`
/// block the next step's user message carries (ARCH §3.3). Stdout bytes
/// round-trip through lossy UTF-8 — the harness wraps a tool's stdout as
/// a single `Content::Text` per §3.3.
fn outcome_to_tool_result(tool_use_id: &str, outcome: &ToolOutcome) -> Content {
    Content::ToolResult {
        tool_use_id: tool_use_id.to_string(),
        content: vec![Content::Text(
            String::from_utf8_lossy(&outcome.content).into_owned(),
        )],
        is_error: outcome.is_error,
    }
}
