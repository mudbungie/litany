//! The delivered-child-result and checkpoint-flush seams of the §6
//! binding interpreter — `advance` *is* the interpreter (ARCH §6).
//!
//! - **A delivered child result** ([`interpret_pending`]): a *result
//!   message* (§2.6, `terminal_ref:`) the drain ([`super::drain`]) left in
//!   the inbox. Its lifecycle event is named by the returning child's
//!   **role**, derived from the dispatch commit subject (the single
//!   authoritative home, [`crate::prompt::role`]): `compactor` →
//!   `compactor_return`; `verifier` → the approve/reject verdict split
//!   ([`verifier`]); else `worker_return` (deliver, or the gate-hold).
//!   The `land_compaction` action is additionally gated on the result's
//!   **epitaph value** (§2.6/§2.7): only a `final-response` compactor
//!   lands; any other ending delivers like an ordinary child return
//!   ([`execute_child`]). The gate and the landing itself live in the
//!   [`landing`] submodule.
//! - **A checkpoint flush** ([`run_flush`]): a due `compaction:` clock at a
//!   step boundary runs `worker_flush` → `dispatch(compactor)`. Its
//!   machinery lives in the [`flush`] submodule, re-exported here so the
//!   hop addresses one `child_result::` surface.
//!
//! **Two passes** (§6 gate): verifier verdicts run *before* worker/compactor
//! results, so an approving verifier drains the held worker result and the
//! second pass skips that consumed message. Circumstance is disk-derived.

mod flush;
mod landing;
mod proposal;
mod verifier;

pub(super) use flush::run_flush;

use super::{drain, transcript, transfer};
use crate::config::{Action, Event, Workflow};
use crate::prompt::{Deps, Error, compactor, inbox, reviewer, role};
use crate::template::GitRunner;
use std::path::{Path, PathBuf};

/// A pending result message (§2.6) awaiting interpretation: the returning
/// child's id (the deposit `<sender>`), its terminal ref, epitaph, the
/// terminal response iff the child spoke, and the inbox file path.
pub(super) struct ChildResult {
    pub(super) child_id: String,
    pub(super) terminal_ref: String,
    pub(super) epitaph: String,
    pub(super) response: Option<String>,
    pub(super) path: PathBuf,
}

/// The terminal ref of a pending deposit that is a **result message this
/// agent must interpret** (§2.6), else `None`. Two conditions: it carries
/// a `terminal_ref:`, *and* its sender is this agent's own child.
///
/// §2.6's return is the **dispatcher's** business — the transfer diffs
/// the child's fork point (`merge-base(HEAD, terminal_ref)`) against the
/// dispatcher's own tree, and the §6 bindings act on a child this agent
/// dispatched. A **reply** delivered anywhere else (a sibling's answer,
/// §2.11) has neither: §2.5's disjoint-write-path guarantee says nothing
/// about a foreign lineage, whose diff would drag that lineage's commits
/// into this tree. It delivers as the ordinary message it is, frontmatter
/// model-visible — how the sender ended, and where its work lives.
pub(super) fn own_result_ref(agent_id: &str, sender: &str, body: &str) -> Option<String> {
    if inbox::parent_of(sender).as_deref() != Some(agent_id) {
        return None;
    }
    transfer::terminal_ref_of(body)
}

/// Whether `agent_id`'s inbox holds any result message (§2.6) — the cheap
/// disk query the hop uses to decide whether to resolve the workflow at
/// all (§6 lazy resolution).
pub(super) fn has_pending_result(workspace: &Path, agent_id: &str) -> Result<bool, Error> {
    let dir = inbox::inbox_dir(workspace, agent_id);
    for msg in drain::pending(&dir)? {
        if own_result_ref(agent_id, &msg.sender, &read_body(&msg.path)?).is_some() {
            return Ok(true);
        }
    }
    Ok(false)
}

/// Interpret every pending result message in `agent_id`'s inbox (§6
/// delivered-child-result circumstance), under the executor lock the hop
/// already holds and against the materialized `worktree`. Verifier
/// verdicts run first (draining any worker result they gate); the
/// remaining worker/compactor results run second, skipping messages a
/// verdict already consumed.
pub(super) fn interpret_pending(
    workspace: &Path,
    agent_id: &str,
    worktree: &Path,
    workflow: &Workflow,
    deps: &Deps<'_>,
) -> Result<(), Error> {
    let dir = inbox::inbox_dir(workspace, agent_id);
    let results = load_results(agent_id, &dir)?;
    let events: Vec<Event> = results
        .iter()
        .map(|cr| child_event(worktree, cr, deps.git))
        .collect::<Result<_, _>>()?;

    for (cr, &event) in results.iter().zip(&events) {
        if matches!(event, Event::VerifierApprove | Event::VerifierReject) {
            for action in child_actions(workflow, event) {
                verifier::execute(
                    &action, event, workspace, agent_id, worktree, cr, &results, deps,
                )?;
            }
        }
    }
    for (cr, &event) in results.iter().zip(&events) {
        if matches!(event, Event::VerifierApprove | Event::VerifierReject) || !cr.path.exists() {
            continue;
        }
        for action in child_actions(workflow, event) {
            execute_child(
                &action, event, workspace, agent_id, worktree, cr, workflow, deps,
            )?;
        }
    }
    Ok(())
}

/// Parse every pending result message in `dir` into a [`ChildResult`];
/// anything [`own_result_ref`] does not classify as this agent's own
/// child's return — an ordinary steering message, a sibling's reply — is
/// skipped, and the drain delivers it as the message it is.
fn load_results(agent_id: &str, dir: &Path) -> Result<Vec<ChildResult>, Error> {
    let mut out = Vec::new();
    for msg in drain::pending(dir)? {
        let body = read_body(&msg.path)?;
        let Some(terminal_ref) = own_result_ref(agent_id, &msg.sender, &body) else {
            continue;
        };
        let (epitaph, response) = split_frontmatter(&body);
        out.push(ChildResult {
            child_id: msg.sender,
            terminal_ref,
            epitaph,
            response,
            path: msg.path,
        });
    }
    Ok(out)
}

/// Read a deposited message body, mapping I/O to [`Error::Io`].
fn read_body(path: &Path) -> Result<String, Error> {
    std::fs::read_to_string(path).map_err(Error::Io)
}

/// Split a result message into its `epitaph` frontmatter value and its
/// body (the terminal response, `None` when the child never spoke).
/// `pub(super)` for the §2.11 release rule's warrant replay
/// ([`super::driver`]), which reads a racing result's epitaph value.
pub(super) fn split_frontmatter(body: &str) -> (String, Option<String>) {
    let mut lines = body.lines();
    if lines.next() != Some("---") {
        return (String::new(), None);
    }
    let mut epitaph = String::new();
    for line in lines.by_ref() {
        if line == "---" {
            break;
        }
        if let Some(v) = line.strip_prefix("epitaph:") {
            epitaph = v.trim().to_string();
        }
    }
    let rest = lines.collect::<Vec<_>>().join("\n");
    let response = (!rest.trim().is_empty()).then_some(rest);
    (epitaph, response)
}

/// Name the lifecycle event of a returning child by its role (§6), derived
/// from the child's dispatch commit subject at its terminal ref (the
/// single authoritative home, [`role`]). `compactor` → `compactor_return`;
/// `verifier` → its verdict (approve/reject, [`verifier::verdict`]); every
/// other role → `worker_return` (deliver, or the gate hold).
fn child_event(worktree: &Path, cr: &ChildResult, git: &dyn GitRunner) -> Result<Event, Error> {
    let derived = role::derive(worktree, &cr.terminal_ref, &cr.child_id, git)?;
    Ok(match derived.as_deref() {
        Some(compactor::COMPACTOR_ROLE) => Event::CompactorReturn,
        Some(reviewer::REVIEWER_ROLE) => Event::ReviewerReturn,
        Some(verifier::VERIFIER_ROLE) => verifier::verdict(cr),
        _ => Event::WorkerReturn,
    })
}

/// The actions bound to a child-result `event`, or its §2.6/§6 baseline
/// default when unbound (severable, `docs/PRINCIPLES.md`).
fn child_actions(workflow: &Workflow, event: Event) -> Vec<Action> {
    let bound = workflow.actions_for(event);
    if !bound.is_empty() {
        return bound;
    }
    match event {
        Event::CompactorReturn => vec![Action::LandCompaction],
        Event::ReviewerReturn => vec![Action::StageProposal],
        Event::VerifierReject => vec![Action::Dispatch {
            role: crate::prompt::WORKER_ROLE.to_string(),
            with: Some(verifier::FEEDBACK.to_string()),
            mode: None,
        }],
        _ => vec![Action::DeliverResult],
    }
}

/// Execute one worker/compactor-result action. `dispatch(verifier)` opens
/// the gate ([`verifier::dispatch`]); `gate_return_on` is the hold itself
/// (a no-op leaving the result in the inbox); `deliver_result` /
/// `land_compaction` are Ball-1. Other actions here are declined loudly.
///
/// **`land_compaction` is epitaph-gated** (§2.6, §2.7): only a
/// compactor that ended on `final-response` completed a compaction pass,
/// so only that epitaph lands. Any other value (`died`, `stopped`,
/// `budget-exhausted`) means the pass never finished — its branch may
/// hold partial `mark_for_deletion` state, and landing it is exactly the
/// corrupted-context outcome the deletion-only toolset exists to rule
/// out. Such a return lands nothing and is instead delivered like any
/// child's (§2.7: "surfaced for user review like any other child
/// failure"): the parent sees the epitaph in its transcript and the
/// branch simply continues uncompacted.
#[allow(clippy::too_many_arguments)] // one action, every fact it executes on
fn execute_child(
    action: &Action,
    event: Event,
    workspace: &Path,
    agent_id: &str,
    worktree: &Path,
    cr: &ChildResult,
    workflow: &Workflow,
    deps: &Deps<'_>,
) -> Result<(), Error> {
    match action {
        Action::Dispatch { role, .. } if role == verifier::VERIFIER_ROLE => {
            verifier::dispatch(workspace, agent_id, worktree, cr, deps)
        }
        Action::GateReturnOn { .. } => Ok(()),
        Action::DeliverResult => deliver_result(worktree, agent_id, cr, deps.git),
        Action::LandCompaction if landing::qualifies(cr) => {
            landing::land(worktree, agent_id, cr, workflow, deps.git)
        }
        Action::StageProposal if landing::qualifies(cr) => {
            proposal::stage(workspace, worktree, cr, deps)
        }
        Action::LandCompaction | Action::StageProposal => {
            deliver_result(worktree, agent_id, cr, deps.git)
        }
        other => Err(Error::ActionUnsupported {
            action: format!("{other:?}"),
            event: event.as_str(),
        }),
    }
}

/// `deliver_result` (§2.6): apply the child's work-product transfer, then
/// move its result message into the transcript. `pub(super)` so the
/// verifier-approve executor drains the held worker result the same way.
pub(super) fn deliver_result(
    worktree: &Path,
    agent_id: &str,
    cr: &ChildResult,
    git: &dyn GitRunner,
) -> Result<(), Error> {
    transfer::apply(worktree, &cr.child_id, &cr.terminal_ref, git)?;
    transcript::deliver_message(worktree, agent_id, &cr.child_id, &cr.path, git)
}

#[cfg(test)]
mod tests;
