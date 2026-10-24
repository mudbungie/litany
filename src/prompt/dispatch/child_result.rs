//! The delivered-child-result and checkpoint-flush seams of the §6
//! binding interpreter — `advance` *is* the interpreter (ARCH §6).
//!
//! - **A delivered child result** ([`interpret_pending`]): a *result
//!   message* (§2.6, `terminal_ref:`) the drain ([`super::drain`]) left in
//!   the inbox. Its event is derived by [`circumstance`] from the returning
//!   child's **role** (the dispatch commit subject, the single
//!   authoritative home, [`crate::prompt::role`]) and its leading word:
//!   `<role>_return`, a bound `<role>_<token>`, or — for a gating child
//!   — the gate's met token or `<role>_reject` ([`verifier`];
//!   `docs/DESIGN_PROCEDURES.md` §5).
//!   The `land_compaction` action is additionally gated on the result's
//!   **epitaph value** (§2.6/§2.7): only a `final-response` compactor
//!   lands; any other ending delivers like an ordinary child return
//!   ([`execute_child`]). The gate and the landing itself live in the
//!   [`landing`] submodule, beside the generic `land_response(<file>)`
//!   (`docs/DESIGN_PROCEDURES.md` §4), gated the same way.
//! - **A checkpoint flush** ([`run_flush`]): a due `compaction:` clock at a
//!   step boundary runs `worker_flush` → `dispatch(compactor)`. Its
//!   machinery lives in the [`flush`] submodule, re-exported here so the
//!   hop addresses one `child_result::` surface.
//!
//! **Two passes** (§6 gate): gating children's verdicts run *before* the
//! other results, so a met gate drains the held result and the second
//! pass skips that consumed message. Circumstance is disk-derived.

mod circumstance;
mod flush;
mod landing;
mod proposal;
mod verifier;

pub(super) use flush::run_flush;

use super::{drain, transcript, transfer};
use crate::config::{Action, Event, Workflow};
use crate::prompt::{Deps, Error, WORKER_ROLE, compactor, inbox, reviewer, workflow_actions};
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
/// already holds and against the materialized `worktree`. Gating
/// children's verdicts run first (draining any result they gate); the
/// remaining results run second, skipping messages a verdict already
/// consumed.
pub(super) fn interpret_pending(
    workspace: &Path,
    agent_id: &str,
    worktree: &Path,
    workflow: &Workflow,
    deps: &Deps<'_>,
) -> Result<(), Error> {
    let dir = inbox::inbox_dir(workspace, agent_id);
    let results = load_results(agent_id, &dir)?;
    let circumstances: Vec<circumstance::Circumstance> = results
        .iter()
        .map(|cr| circumstance::derive(worktree, cr, &results, workflow, deps.git))
        .collect::<Result<_, _>>()?;

    for (cr, c) in results.iter().zip(&circumstances) {
        let Some(gated) = &c.gated else {
            continue;
        };
        for action in child_actions(workflow, &c.event, Some(&gated.role)) {
            verifier::execute(
                &action,
                &c.event,
                workspace,
                agent_id,
                worktree,
                cr,
                &results[gated.index],
                deps,
            )?;
        }
    }
    for (cr, c) in results.iter().zip(&circumstances) {
        if c.gated.is_some() || !cr.path.exists() {
            continue;
        }
        for action in child_actions(workflow, &c.event, None) {
            execute_child(
                &action, &c.event, workspace, agent_id, worktree, cr, workflow, deps,
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

/// The actions bound to a child-result `event`, or its §2.6/§6 baseline
/// default when unbound (severable, `docs/PRINCIPLES.md`): a compactor's
/// return lands, a reviewer's stages, a gate's unmet outcome re-dispatches
/// the gated role (`gated_role`, else the worker) with the verdict as
/// feedback, and every other return delivers.
fn child_actions(workflow: &Workflow, event: &Event, gated_role: Option<&str>) -> Vec<Action> {
    let bound = workflow.actions_for(event);
    if !bound.is_empty() {
        return bound;
    }
    match event {
        Event::Return(role) if role == compactor::COMPACTOR_ROLE => vec![Action::LandCompaction],
        Event::Return(role) if role == reviewer::REVIEWER_ROLE => vec![Action::StageProposal],
        Event::Reject(role) => vec![Action::Dispatch {
            role: gated_role.unwrap_or(WORKER_ROLE).to_string(),
            with: Some(verifier::feedback_token(role)),
            mode: None,
        }],
        _ => vec![Action::DeliverResult],
    }
}

/// Execute one child-result action. `dispatch(<role>)` at a child's
/// return forks that role off the returning child's terminal ref
/// ([`verifier::dispatch`] — the gate's opening, for any role);
/// `gate_return_on` is the hold itself (a no-op leaving the result in the
/// inbox); the landings and `deliver_result` consume or deliver; the two
/// ref marks (`notify_ui`, `mark_abandoned`) **mark the dispatcher** — a
/// verdict about this branch's situation (`guard_injected`) is what a
/// mark is for (`docs/DESIGN_PROCEDURES.md` §7) — and touch the result
/// not at all, exactly as the hold does: what consumes it is the landing
/// or the delivery bound beside them. The match is exhaustive: every
/// member of the closed set has a meaning at a child's return.
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
    event: &Event,
    workspace: &Path,
    agent_id: &str,
    worktree: &Path,
    cr: &ChildResult,
    workflow: &Workflow,
    deps: &Deps<'_>,
) -> Result<(), Error> {
    match action {
        Action::Dispatch { role, .. } => {
            verifier::dispatch(role, workspace, agent_id, worktree, cr, deps)
        }
        Action::GateReturnOn { .. } => Ok(()),
        Action::DeliverResult => deliver_result(worktree, agent_id, cr, deps.git),
        Action::LandCompaction if landing::qualifies(cr) => {
            landing::land(worktree, agent_id, cr, workflow, deps.git)
        }
        Action::StageProposal if landing::qualifies(cr) => {
            proposal::stage(workspace, worktree, cr, deps)
        }
        Action::LandResponse { file } if landing::qualifies(cr) => {
            landing::land_response(worktree, cr, file, deps.git)
        }
        Action::LandCompaction | Action::StageProposal | Action::LandResponse { .. } => {
            deliver_result(worktree, agent_id, cr, deps.git)
        }
        Action::MarkAbandoned | Action::NotifyUi => {
            workflow_actions::execute(action, event, worktree, agent_id, deps.git)
        }
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
