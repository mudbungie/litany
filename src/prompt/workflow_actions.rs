//! The workflow binding interpreter (ARCH §6 "The binding interpreter").
//!
//! `litany advance` *is* the interpreter (§6): one hop derives its
//! circumstance from disk and runs the bindings that match, holding no
//! cursor. This module is two seams of that evaluation:
//!
//! - the **terminal-lifecycle seam** — when a hop's step reached a
//!   terminal event, the epitaph names a lifecycle event
//!   ([`lifecycle_event`]) and the workflow's bindings for it are executed
//!   ([`run_terminal_bindings`]);
//! - the **self-firing seam** — the per-step hooks and `user_message`
//!   ([`run_step_hook`]; `docs/DESIGN_PROCEDURES.md` §2–§3), the
//!   circumstances a branch's own hop fires with no child returning.
//!
//! Two git-native **ref-mark** actions run at either seam, the same
//! pattern as the §2.6 `budget-exhausted` / `conflicted` marks and read
//! (never pushed) by the frontend (§3.5): `mark_abandoned` writes
//! `refs/litany/abandoned/<agent-id>` — a *non-derivable policy
//! assertion* ("this stopped branch will not be retried", distinct from
//! the derivable `stopped` classification, so it is a fact with a home,
//! not a mirror of one, `docs/PRINCIPLES.md` Single source of truth) —
//! and `notify_ui` writes `refs/litany/notify/<agent-id>`, the mark a
//! frontend surfaces as a user-facing notification. At the self-firing
//! seam `dispatch(<role>)` forks a **procedure child** off the tip the
//! hook observed ([`crate::prompt::procedure::dispatch`]). Every other
//! action in the closed set is declined loudly ([`Error::ActionUnsupported`])
//! rather than silently dropped — a landing is minted from a child's
//! return, and neither seam has one (`docs/PRINCIPLES.md` "Decline
//! illegal operations").

use crate::config::version::Version;
use crate::config::{Action, Event, Workflow};
use crate::prompt::inbox::Epitaph;
use crate::prompt::{Deps, Error, procedure};
use crate::template::GitRunner;
use std::path::Path;

/// Ref prefix for the `mark_abandoned` action (ARCH §6): a git-native,
/// per-agent-id policy mark at the branch tip.
const ABANDONED_REF_PREFIX: &str = "refs/litany/abandoned/";
/// Ref prefix for the `notify_ui` action (ARCH §6, §3.5): a git-native,
/// per-agent-id user-facing-notification mark at the branch tip.
const NOTIFY_REF_PREFIX: &str = "refs/litany/notify/";

/// The lifecycle event a branch's *own* terminal names, by epitaph value
/// (ARCH §6 "Circumstance is derived from disk"; §2.6 — code branches on
/// the epitaph's value, never on message shape). Only `stopped` has a
/// distinct terminal-lifecycle binding today; `final-response` and
/// `budget-exhausted` terminate through the ordinary exit protocol and
/// name no event here, so they yield `None`.
pub(super) fn lifecycle_event(epitaph: Epitaph) -> Option<Event> {
    match epitaph {
        Epitaph::Stopped => Some(Event::BranchStopped),
        Epitaph::FinalResponse | Epitaph::BudgetExhausted | Epitaph::Died => None,
    }
}

/// Evaluate the workflow's bindings for the lifecycle event this branch's
/// terminal named (if any) and run each in declared order. A terminal
/// with no lifecycle event, or an event with no bindings, is a no-op —
/// the general path with empty inputs, not a special case. `worktree` is
/// the branch's materialized tree whose checked-out `HEAD` is the tip the
/// ref marks point at (§2.3).
pub(super) fn run_terminal_bindings(
    workflow: &Workflow,
    epitaph: Epitaph,
    worktree: &Path,
    agent_id: &str,
    git: &dyn GitRunner,
) -> Result<(), Error> {
    let Some(event) = lifecycle_event(epitaph) else {
        return Ok(());
    };
    for action in workflow.actions_for(&event) {
        execute(&action, &event, worktree, agent_id, git)?;
    }
    Ok(())
}

/// Evaluate the workflow's bindings for a **self-firing** event —
/// `pre_step` / `post_step` / `on_tool_return` (ARCH §6, bracketing the
/// model call in [`crate::prompt::dispatch`]) or `user_message` (fired
/// where the drain delivered an operator message,
/// `docs/DESIGN_PROCEDURES.md` §3) — and run each in declared order:
/// the two ref marks, and `dispatch(<role>)`, which forks a procedure
/// child off the tip the hook observed. Any other closed-set action is
/// declined loudly. An unbound event is the empty-inputs no-op.
///
/// **A machinery branch runs none of them** (`procedure`, bl-08b4): a
/// procedure child inherits the dispatching branch's workflow, so
/// without this base case its own hook forks the procedure that forked
/// it, without end. `role` is this branch's own, from its dispatch
/// commit (§2.5); `version` is the config's, under which the set is read.
#[allow(clippy::too_many_arguments)] // one seam, every fact it is judged on
pub(crate) fn run_step_hook(
    workspace: &Path,
    agent_id: &str,
    worktree: &Path,
    workflow: &Workflow,
    version: Version,
    role: &str,
    event: Event,
    deps: &Deps<'_>,
) -> Result<(), Error> {
    if procedure::is_machinery(workflow, version, role) {
        return Ok(());
    }
    for action in procedure::self_firing_actions(workflow, &event) {
        match &action {
            Action::Dispatch { role, .. } => {
                procedure::dispatch(role, workspace, agent_id, worktree, None, deps)?;
            }
            other => execute(other, &event, worktree, agent_id, deps.git)?,
        }
    }
    Ok(())
}

/// Execute one action at a terminal-lifecycle `event`, or a non-dispatch
/// one at a self-firing event. The shipped subset is the two ref marks;
/// every remaining member of the closed set is declined loudly — a
/// landing or a hold is minted from a child's return, and there is none
/// at a branch's own terminal or hook.
///
/// The match is **exhaustive by construction** — no `_` arm. That is the
/// totality guarantee for the vocabulary: a new [`Action`] variant cannot
/// be added without landing here and deciding, on the spot, whether it
/// executes or is an acknowledged deferral. Vocabulary that can reach
/// neither arm (`spawn_root_agent`, `spawn_exchange`) is not in the enum
/// at all — it is declined at parse (`config::action`).
pub(super) fn execute(
    action: &Action,
    event: &Event,
    worktree: &Path,
    agent_id: &str,
    git: &dyn GitRunner,
) -> Result<(), Error> {
    match action {
        Action::MarkAbandoned => mark_ref(ABANDONED_REF_PREFIX, worktree, agent_id, git),
        Action::NotifyUi => mark_ref(NOTIFY_REF_PREFIX, worktree, agent_id, git),
        deferred @ (Action::Dispatch { .. }
        | Action::GateReturnOn { .. }
        | Action::DeliverResult
        | Action::LandCompaction
        | Action::StageProposal
        | Action::LandResponse { .. }) => Err(Error::ActionUnsupported {
            action: format!("{deferred:?}"),
            event: event.key(),
        }),
    }
}

/// Write a git-native marker ref `<prefix><agent-id>` at the branch tip,
/// run inside `worktree` — whose checked-out branch *is* the agent's
/// branch (§2.3), so `HEAD` is the tip with no ref-name round trip. The
/// same shape as [`crate::prompt::budget::mark_exhausted`]; state lives in
/// git, never a sidecar (`docs/PRINCIPLES.md` Single source of truth).
fn mark_ref(
    prefix: &str,
    worktree: &Path,
    agent_id: &str,
    git: &dyn GitRunner,
) -> Result<(), Error> {
    let ref_name = format!("{prefix}{agent_id}");
    git.run(worktree, &["update-ref", ref_name.as_str(), "HEAD"])
        .map_err(|source| Error::Git {
            op: "workflow mark update-ref",
            source,
        })
}

#[cfg(test)]
mod tests;
