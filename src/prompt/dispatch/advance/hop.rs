//! One warranted step of a `litany advance` hop (§6 hop step 4).
//!
//! The same §2.3 step-loop body [`super::super::run_exchange`] drives,
//! re-rooted on disk instead of loop locals: the step sequence is
//! derived from the `steps/` listing, the goal from the pinned
//! `goal.md` (§2.8), and the toolset from the branch's committed
//! `descriptions/**` — nothing rides the exec baton but the lock fd.
//! The §2.9 stop check points bracket the model call *and* follow the
//! tool window, so a stop caught anywhere in the hop becomes the
//! `stopped`-epitaph terminal rather than riding into a successor.

use super::super::{
    assembler, canonical, child_result, model_call, result_deposit, step_commit, stop_signal,
    terminal, tool_step, tools, transcript,
};
use crate::config::Event;
use crate::prompt::inbox::Epitaph;
use crate::prompt::resolve::WorkerConfig;
use crate::prompt::step::{RESPONSE_FILE, STAGING_FILE, StepMeta, next_step_seq, step_dir_rel};
use crate::prompt::workflow_actions;
use crate::prompt::{Deps, Error};
use brazen::Content;
use model_call::ModelCall;
use std::path::Path;

/// How one warranted step ended.
pub(super) enum StepOutcome {
    /// The step emitted `tool_use` and every tool ran and committed its
    /// result: the successor hop's model call is now due (§6).
    ToolsRan,
    /// A terminal event: final response (deposited here, §2.6), stop
    /// (§2.9), or budget exhaustion (§6, deposited at the boundary
    /// check). The caller runs the §2.11 exit protocol.
    Terminal(Epitaph),
    /// The configured control held an invocation (§3.3 *Tool control*):
    /// the branch parks mid-tool-window — no terminal, no deposit — and
    /// the caller releases the lease and exits. The hold mark plus the
    /// unpaired tail are the whole state.
    Held,
}

/// Run one step against `worktree` (the branch's materialized tree,
/// post-drain). Mirrors the `run_exchange` loop body — one step
/// machinery, two drivers (§6 shipped-state note).
pub(super) fn step(
    workspace: &Path,
    agent_id: &str,
    worktree: &Path,
    cfg: &WorkerConfig,
    deps: &Deps<'_>,
) -> Result<StepOutcome, Error> {
    // §2.9 step-3 check point: a stop delivered before the model call.
    if stop_signal::stopped(deps.stop) {
        return Ok(StepOutcome::Terminal(Epitaph::Stopped));
    }
    let resolved = cfg.as_resolved();
    // §6 budget check (deposits + marks the ref on exhaustion, §2.9).
    if terminal::budget_exhausted(
        workspace,
        agent_id,
        agent_id,
        worktree,
        &resolved.budgets,
        deps,
    )? {
        return Ok(StepOutcome::Terminal(Epitaph::BudgetExhausted));
    }

    // §6 per-step hook: `pre_step` fires before the model call is issued.
    workflow_actions::run_step_hook(&cfg.workflow, Event::PreStep, worktree, agent_id, deps.git)?;

    // §2.8 / §2.3: goal and name are pinned worktree files, re-read per
    // hop — the system slot is composed from the tree, like every other
    // part of the request (§5.1 one input), so nothing rides the baton.
    let goal = std::fs::read_to_string(worktree.join(step_commit::GOAL_FILE))?;
    let name = crate::workspace::agent_name::in_worktree(worktree);
    let system_with_goal = step_commit::compose_system(&goal, name.as_deref(), &resolved.soul);
    let call = ModelCall {
        adapter: deps.adapter,
        sleeper: deps.sleeper,
        binary: &resolved.binary,
        provider_row: resolved.provider_row,
        retry: resolved.retry,
        stop: deps.stop,
        expect_handshake: resolved.expect_handshake,
    };

    let step_seq = next_step_seq(workspace, agent_id)?;
    let commit_sha = step_commit::read_branch_tip(worktree, deps)?;
    // §2.3 / §5: assemble the model-facing history from the tree — the
    // §5.2 head/body under the role's manifest rules, then the
    // transcript tail — one code path for running, retry, and replay.
    let messages = assembler::assemble(worktree, resolved.manifest)?;
    // Everything this request declares (§3.3, §4.3, §2.7 — see [`tools`]):
    // the role's elected tools, the compactor's injected pair, and the
    // closure over the assembled history. A compactor inherits the
    // dispatching branch's transcript (§2.3), so that last part is not an
    // edge case for it — it is the standing case.
    let tools = tools::compose(
        worktree,
        resolved.grant.tools,
        &messages,
        &tools::injected(resolved.grant.role, deps.tool_executor, workspace, agent_id),
    )?;
    let request = canonical::build_request(
        resolved.model_id,
        &system_with_goal,
        messages,
        tools,
        step_commit::DEFAULT_MAX_TOKENS,
        resolved.effort,
    );
    let request_value =
        serde_json::to_value(&request).expect("CanonicalRequest is always serializable");
    let step_dir_rel_str = step_dir_rel(agent_id, step_seq);
    step_commit::write_request(workspace, &step_dir_rel_str, &request_value)?;

    let request_bytes =
        serde_json::to_vec(&request).expect("CanonicalRequest is always serializable");
    let started_at = deps.clock.now_iso8601();
    let response_path = workspace.join(&step_dir_rel_str).join(RESPONSE_FILE);
    let call_outcome = model_call::run(&call, &request_bytes, &response_path);
    // §2.9 step-3 check point: a stop during the call killed `bz` — the
    // flag classifies, not the error's shape ([`model_call`]). Same rule
    // as `run_exchange`.
    if stop_signal::stopped(deps.stop) {
        return Ok(StepOutcome::Terminal(Epitaph::Stopped));
    }
    call_outcome?;
    let ended_at = deps.clock.now_iso8601();

    step_commit::write_meta(
        workspace,
        &step_dir_rel_str,
        &StepMeta {
            commit: commit_sha,
            started_at,
            ended_at,
        },
    )?;

    // Transcript writer (§2.3): seal-and-rename the staging entry.
    let staging_path = workspace.join(&step_dir_rel_str).join(STAGING_FILE);
    let assistant_content = transcript::commit_assistant(
        worktree,
        agent_id,
        resolved.model_id,
        &staging_path,
        deps.git,
    )?;

    // §6 per-step hook: `post_step` fires after the model call returns and
    // its output is committed, before any tool executes.
    workflow_actions::run_step_hook(&cfg.workflow, Event::PostStep, worktree, agent_id, deps.git)?;

    // No `tool_use` block is terminal (§2.5): deposit a `final-response`
    // result, body iff the agent spoke (§2.6). A no-op for a root.
    if !assistant_content
        .iter()
        .any(|b| matches!(b, Content::ToolUse { .. }))
    {
        let response = result_deposit::terminal_text(&assistant_content);
        result_deposit::deposit_terminal(
            workspace,
            agent_id,
            worktree,
            Epitaph::FinalResponse,
            response.as_deref(),
            deps,
        )?;
        return Ok(StepOutcome::Terminal(Epitaph::FinalResponse));
    }

    // §2.5 pairing: run each tool_use, committing its tool_result as a
    // transcript entry (§2.3); the successor re-assembles from the tree.
    // §2.9 step-3 check point: a stop landing in this tool-execution
    // window (the group SIGTERM felled the running tool) — or caught by
    // the flag after the tools resolved — terminates here, never riding
    // the baton into a successor (the flag would evaporate across exec).
    // A window the control held (§3.3 *Tool control*) parks instead.
    match tool_step::run_tool_calls(
        workspace,
        worktree,
        agent_id,
        &resolved,
        &step_dir_rel_str,
        &assistant_content,
        deps,
    )? {
        tool_step::ToolWindow::Stopped => return Ok(StepOutcome::Terminal(Epitaph::Stopped)),
        tool_step::ToolWindow::Held => return Ok(StepOutcome::Held),
        tool_step::ToolWindow::Completed => {}
    }
    if stop_signal::stopped(deps.stop) {
        return Ok(StepOutcome::Terminal(Epitaph::Stopped));
    }

    // §6 per-step hook: `on_tool_return` fires once the step's tool calls
    // have resolved and committed (advance-native, this hop's tool window).
    workflow_actions::run_step_hook(
        &cfg.workflow,
        Event::OnToolReturn,
        worktree,
        agent_id,
        deps.git,
    )?;

    // §2.7/§6 checkpoint: this step's commits landed, so the tip is C.
    // If the `compaction:` clock is due, run the `worker_flush` bindings
    // (dispatch a compactor off C). Config-only — a branch with no
    // `compaction:` block never fires, so this is a no-op for it.
    child_result::run_flush(workspace, agent_id, worktree, &cfg.workflow, deps)?;
    Ok(StepOutcome::ToolsRan)
}
