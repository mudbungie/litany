//! Root-conversation branch orchestration (ARCH §2.3–§2.10).
//!
//! [`run_exchange`] executes a single root conversation: spawn branch
//! `agents/<conv-id>` off the start's fork point (§2.2–§2.3 — a config
//! lineage's head by default, any ref with `--from`); seed the working
//! directory the start named (§3.3, `--cwd`); write the step-1 dispatch
//! commit (§2.2, §2.10); then the step loop (§2.5) —
//! drain the inbox ([`drain`], §2.11), interpret delivered child results
//! and the compaction checkpoint at each boundary (the §6 prompt→advance
//! collapse, [`child_result`]), drive the model call through the retry
//! loop ([`model_call`], §4.4), re-assemble history from the read-state
//! commit ([`assembler`], §2.3, §5), and loop on a settled `tool_use`.
//! Every terminal deposits a result message ([`result_deposit`], §2.6,
//! no-op for a root), then runs the shared §2.11 terminal tail
//! (`terminal::conclude`): terminal-lifecycle bindings (§6), lease release
//! through the §2.11 release rule (a deposit that raced the last drain is
//! launched for, [`driver`]), and the epitaph-valued exit launches at own
//! agent and at the parent the deposit revived.

pub mod advance;
mod assembler;
mod child_result;
mod drain;
pub mod driver;
mod entry;
mod model_call;
mod resolved;
mod result_deposit;
mod staging;
mod step_commit;
pub mod stop_signal;
mod terminal;
mod tool_step;
mod tools;
mod transcript;
mod transfer;

pub use model_call::{RealSleeper, Sleeper};
pub(super) use resolved::Resolved;
pub(crate) use step_commit::inherited::prune_inherited_dialog;
pub(crate) use step_commit::{Grant, Undescribed, require_described, trim_to_context};
pub use stop_signal::{flag as stop_flag, install as install_stop_handler};
pub(crate) use transcript::MESSAGES_DIR;

use super::inbox::{self, Epitaph};
use super::step::{RESPONSE_FILE, STAGING_FILE, StepMeta, step_dir_rel};
use super::{Deps, Error};
use assembler::assemble;
use brazen::Content;
use model_call::ModelCall;
use std::path::Path;
use step_commit::{
    DEFAULT_MAX_TOKENS, commit_dispatch, compose_system, read_branch_tip, spawn_branch,
    write_dispatch_files, write_meta, write_request,
};
use tool_step::run_tool_calls;

/// Drive one root conversation against an already-resolved config,
/// forked off `fork_point` (§2.3 — the ref the start named, resolved by
/// [`super::fork_point`]). Returns the branch name so the caller can
/// surface it on stdout.
#[allow(clippy::too_many_arguments)]
pub(super) fn run_exchange(
    repo: &Path,
    user_message: &str,
    fork_point: &str,
    name: Option<&str>,
    pins: &crate::prompt::PinnedDocs,
    cwd: Option<&Path>,
    resolved: &Resolved<'_>,
    deps: &Deps<'_>,
) -> Result<String, Error> {
    let ts = deps.clock.now_compact();
    let short_id = deps.id_gen.short();
    let conv_id = format!("{ts}-{short_id}");
    let branch_name = conv_id.clone();
    let worktree_path = crate::workspace::agent_worktree(repo, &conv_id);

    // Executor lock (§2.11): acquire the branch's inbox lease before any
    // work, held for the whole loop and kernel-released on exit. Losing
    // the acquire means another driver owns this branch — clean no-op
    // (Writer/driver totality); a fresh root always wins (unique conv-id).
    let inbox = inbox::inbox_dir(repo, &conv_id);
    let executor_lock = match inbox::try_acquire(&inbox).map_err(|source| Error::ExecutorLock {
        path: inbox.clone(),
        source,
    })? {
        Some(guard) => guard,
        None => return Ok(branch_name),
    };

    super::seed_cwd(repo, &conv_id, cwd, deps.git)?;

    spawn_branch(repo, &worktree_path, &conv_id, fork_point, deps)?;

    // The initial user message enters through the front door (§2.4,
    // §2.11): deposited into this agent's own inbox, delivered by the
    // step-1 drain — the same path any reprompt takes.
    inbox::deposit(repo, &conv_id, inbox::USER_SENDER, user_message, deps.clock)?;

    // The system slot (§2.3, §5.2): goal, identity, soul. The goal and
    // the name are the ones this start was given — the same values the
    // dispatch commit below writes to `goal.md` and `name`, so the slot
    // and the tree state one fact, not two (§2.10 replay re-reads the
    // tree and reproduces this byte-for-byte).
    let system_with_goal = compose_system(user_message, name, &resolved.soul);

    let call = ModelCall {
        adapter: deps.adapter,
        sleeper: deps.sleeper,
        binary: &resolved.binary,
        provider_row: resolved.provider_row,
        retry: resolved.retry,
        stop: deps.stop,
        expect_handshake: resolved.expect_handshake,
    };

    let mut step_seq: u32 = 1;
    let mut exhausted = false;
    // §2.9 step 3: set when a check point sees the SIGTERM handler flag.
    let mut stopped = false;
    // What the latest drain deliberately left pending — the §2.11
    // release rule's diff base at the tail (every loop exit follows an
    // assignment, so this is never unset).
    let mut seen;
    loop {
        if step_seq == 1 {
            write_dispatch_files(&worktree_path, user_message, &resolved.soul, pins)?;
            commit_dispatch(&worktree_path, &conv_id, name, pins, resolved, deps)?;
        }

        // Step-boundary drain (§2.11 *Delivery*): move each pending inbox
        // message into the transcript ahead of this step's read-state
        // capture — after the prior step's tool entries, so a message
        // never wedges between paired tool blocks (§2.3).
        seen = drain::drain(&worktree_path, &inbox, &conv_id, deps.git)?.left;

        // §6 prompt→advance collapse: interpret delivered child results
        // (deliver_result / land_compaction / verifier gate) at the same
        // boundary `litany advance` does — empty-inputs no-op for a root.
        child_result::interpret_pending(repo, &conv_id, &worktree_path, resolved.workflow, deps)?;

        let commit_sha = read_branch_tip(&worktree_path, deps)?;

        // §2.9 step 3 check point: a stop between steps (or during a prior
        // step's tool work) is caught here, before the next model call.
        if stop_signal::stopped(deps.stop) {
            stopped = true;
            break;
        }

        // §6 budget check (deposits + marks the ref on exhaustion, §2.9).
        if terminal::budget_exhausted(
            repo,
            &conv_id,
            &branch_name,
            &worktree_path,
            &resolved.budgets,
            deps,
        )? {
            exhausted = true;
            break;
        }

        // §2.3 / §5: assemble the model-facing history from the read-state
        // commit's tree — §5.2 head/body under the role's manifest rules,
        // then the transcript tail — one path for running, retry, replay.
        let messages = assemble(&worktree_path, resolved.manifest)?;
        let tools = tools::compose(
            &worktree_path,
            resolved.grant.tools,
            &messages,
            &tools::injected(resolved.grant.role, deps.tool_executor),
        )?;
        let request = model_call::build_request(
            resolved.model_id,
            &system_with_goal,
            messages,
            tools,
            DEFAULT_MAX_TOKENS,
        );
        let request_value =
            serde_json::to_value(&request).expect("CanonicalRequest is always serializable");
        let step_dir_rel_str = step_dir_rel(&conv_id, step_seq);
        write_request(repo, &step_dir_rel_str, &request_value)?;

        let request_bytes =
            serde_json::to_vec(&request).expect("CanonicalRequest is always serializable");
        let started_at = deps.clock.now_iso8601();
        let response_path = repo.join(&step_dir_rel_str).join(RESPONSE_FILE);
        let call_outcome = model_call::run(&call, &request_bytes, &response_path);
        // §2.9 step 3 check point: a stop during the call killed `bz`. The
        // flag classifies, not the error's shape ([`model_call`]) — swallow
        // whatever it surfaced (on-disk signature untouched) and exit.
        if stop_signal::stopped(deps.stop) {
            stopped = true;
            break;
        }
        call_outcome?;
        let ended_at = deps.clock.now_iso8601();

        write_meta(
            repo,
            &step_dir_rel_str,
            &StepMeta {
                commit: commit_sha,
                started_at,
                ended_at,
            },
        )?;

        // Transcript writer (§2.3): seal-and-rename the staging entry to
        // `messages/NNN-<model-id>.json` (origin = authoring model) + commit.
        let staging_path = repo.join(&step_dir_rel_str).join(STAGING_FILE);
        let assistant_content = transcript::commit_assistant(
            &worktree_path,
            &conv_id,
            resolved.model_id,
            &staging_path,
            deps.git,
        )?;

        // No `tool_use` block is terminal (§2.5): deposit a `final-response`
        // result, response body iff the agent spoke (§2.6). No-op for a root.
        if !assistant_content
            .iter()
            .any(|b| matches!(b, Content::ToolUse { .. }))
        {
            let response = result_deposit::terminal_text(&assistant_content);
            result_deposit::deposit_terminal(
                repo,
                &conv_id,
                &worktree_path,
                Epitaph::FinalResponse,
                response.as_deref(),
                deps,
            )?;
            break;
        }

        // §2.5 pairing: run each tool_use, committing its tool_result as
        // a transcript entry (§2.3). A stop felling the window breaks for
        // the same stopped-deposit exit as the model-call window (§2.9
        // step 3); a window the configured control held (§3.3 *Tool
        // control*) parks instead — no terminal, no deposit, the hold
        // mark and the unpaired tail the whole state, the lease released
        // through the §2.11 release rule for a later `litany advance` to
        // resume by re-adjudication.
        let window = run_tool_calls(
            repo,
            &worktree_path,
            &conv_id,
            resolved,
            &step_dir_rel_str,
            &assistant_content,
            deps,
        )?;
        match window {
            tool_step::ToolWindow::Stopped => {
                stopped = true;
                break;
            }
            tool_step::ToolWindow::Held => {
                driver::release_then_reprobe(executor_lock, repo, &conv_id, &seen, deps.launcher);
                return Ok(branch_name);
            }
            tool_step::ToolWindow::Completed => {}
        }

        // §6 collapse: the `compaction:` checkpoint clock, same seam as
        // `litany advance` (`worker_flush` → dispatch a compactor off C).
        child_result::run_flush(repo, &conv_id, &worktree_path, resolved.workflow, deps)?;
        step_seq += 1;
    }

    // The shared §2.11 terminal tail ([`terminal::conclude`] — the same
    // sequence as the `litany advance` hop's): finish by epitaph value
    // (a stopped branch deposits its result on the way out; a final
    // response deposited in the loop; an exhausted branch at the boundary
    // check), terminal-lifecycle bindings (§6), release through the
    // release rule (a deposit that raced this loop's last drain launches
    // whatever the epitaph), then the epitaph-valued exit launches. No
    // terminal compaction (§2.7 — the stage is deleted).
    let epitaph = match (stopped, exhausted) {
        (true, _) => Epitaph::Stopped,
        (false, true) => Epitaph::BudgetExhausted,
        (false, false) => Epitaph::FinalResponse,
    };
    terminal::conclude(
        repo,
        &conv_id,
        epitaph,
        resolved.workflow,
        executor_lock,
        &seen,
        deps,
    )?;

    Ok(branch_name)
}
