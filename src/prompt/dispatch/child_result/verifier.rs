//! The **gate** (ARCH §6 "gate_return_on — the delivery-hold"), over any
//! role (`docs/DESIGN_PROCEDURES.md` §5).
//!
//! A gating child is an ordinary child agent (§2.5) whose terminal
//! response is its verdict. The gate is realized without any stored flag:
//!
//! - `dispatch(<role>)` bound on a child's return event forks that role
//!   off the **returning child's terminal ref** ([`dispatch`]) — so it
//!   inherits the work it must judge — while its id stays
//!   `<parent>-<sub>` (it reports to the gating parent). The returning
//!   result is left undelivered in the inbox: the *hold* is that disk
//!   state, "a result present + a child of the gating role dispatched
//!   off it + not yet met", queried, never flagged ([`already_gated`],
//!   the ancestry of the gating child over the held ref).
//! - On the gating child's return, its event is derived by
//!   [`super::circumstance`]: the predicate `gate_return_on(<role>.<token>)`
//!   met ⇒ `<role>_<token>`; anything else ⇒ `<role>_reject` — a total
//!   partition, no third state.
//! - The met event's `deliver_result` drains the held result through the
//!   ordinary delivery (§2.6); `<role>_reject`'s `dispatch(<gated role>,
//!   with: <role>.feedback)` re-dispatches the gated role with the verdict
//!   as steering and discards the rejected result. The held result is
//!   found by ancestry ([`gated_index`]), not a sidecar link.
//!
//! The verifier is the general path with a particular name: its role
//! keeps the one role-owned goal text ([`return_goal`]), because its soul
//! cannot know it is judging a *worker's* response; every other gating
//! role is instructed by the generic procedure goal plus the response.

use super::ChildResult;
use crate::config::{Action, Event};
use crate::prompt::{ChildDispatchRequest, Deps, Error, child_dispatch, procedure, role};
use crate::template::GitRunner;
use std::path::Path;

/// Role name of the shipped verifier (§6). Its soul is `souls/verifier.md`
/// in the governing config commit; the gate is otherwise config-only.
pub(super) const VERIFIER_ROLE: &str = "verifier";

/// The `with:` token naming a gating child's own response as the
/// re-dispatch steering on rejection (`dispatch(worker, with:
/// verifier.feedback)`, §6): `<role>.feedback`.
pub(super) fn feedback_token(role: &str) -> String {
    format!("{role}.feedback")
}

/// Open the gate: fork `role` off the returning child's terminal ref,
/// unless one already gates it ([`already_gated`] — the idempotent, disk-
/// derived hold). The returning result is left in the inbox by the caller.
pub(super) fn dispatch(
    role: &str,
    workspace: &Path,
    agent_id: &str,
    worktree: &Path,
    returning: &ChildResult,
    deps: &Deps<'_>,
) -> Result<(), Error> {
    if already_gated(worktree, agent_id, &returning.terminal_ref, role, deps.git)? {
        return Ok(());
    }
    let goal = return_goal(
        role,
        worktree,
        agent_id,
        returning.response.as_deref().unwrap_or(""),
    )?;
    let req = ChildDispatchRequest {
        repo: workspace,
        parent_branch: agent_id,
        parent_worktree: worktree,
        role,
        goal: &goal,
        name: None,
        fork_point: Some(&returning.terminal_ref),
        cwd: None,
        pins: crate::prompt::PinnedDocs::none(),
    };
    child_dispatch::run_procedure(
        &req,
        deps.git,
        deps.clock,
        deps.id_gen,
        deps.launcher,
        deps.rng,
    )
}

/// Execute a gating child's verdict action (§6): met drains the held
/// result; unmet re-dispatches the gated role with feedback and discards
/// it. `gated` is the held result this child answers for.
#[allow(clippy::too_many_arguments)]
pub(super) fn execute(
    action: &Action,
    event: &Event,
    workspace: &Path,
    agent_id: &str,
    worktree: &Path,
    cr: &ChildResult,
    gated: &ChildResult,
    deps: &Deps<'_>,
) -> Result<(), Error> {
    match (event, action) {
        (Event::Verdict { .. }, Action::DeliverResult) => {
            super::deliver_result(worktree, agent_id, gated, deps.git)?;
            consume(cr)
        }
        (
            Event::Reject(role),
            Action::Dispatch {
                role: next, with, ..
            },
        ) => {
            let feedback = match with.as_deref() {
                None => cr.response.clone().unwrap_or_default(),
                Some(token) if token == feedback_token(role) => {
                    cr.response.clone().unwrap_or_default()
                }
                Some(literal) => literal.to_string(),
            };
            let req = ChildDispatchRequest {
                repo: workspace,
                parent_branch: agent_id,
                parent_worktree: worktree,
                role: next,
                goal: &feedback,
                name: None,
                fork_point: None,
                cwd: None,
                pins: crate::prompt::PinnedDocs::none(),
            };
            child_dispatch::run_procedure(
                &req,
                deps.git,
                deps.clock,
                deps.id_gen,
                deps.launcher,
                deps.rng,
            )?;
            consume(gated)?;
            consume(cr)
        }
        _ => Err(Error::ActionUnsupported {
            action: format!("{action:?}"),
            event: event.key(),
        }),
    }
}

/// The index among `results` of the held result `cr` gates: the pending
/// result whose terminal ref is an ancestor of `cr`'s (`cr` was forked off
/// it, §6). Ancestry is the link — no sidecar.
pub(super) fn gated_index(
    results: &[ChildResult],
    cr: &ChildResult,
    worktree: &Path,
    git: &dyn GitRunner,
) -> Option<usize> {
    results.iter().position(|r| {
        r.child_id != cr.child_id
            && r.path.exists()
            && is_ancestor(worktree, &r.terminal_ref, &cr.terminal_ref, git)
    })
}

/// Whether a child of `role` already gates the result at `held_ref` (§6):
/// an `agents/<parent>-*` child wearing that role that forked off (has as
/// ancestor) that ref. Enumerated from git — the hold is a query.
fn already_gated(
    worktree: &Path,
    parent: &str,
    held_ref: &str,
    role: &str,
    git: &dyn GitRunner,
) -> Result<bool, Error> {
    let pattern = format!("agents/{parent}-*");
    let out = git
        .run_capture(
            worktree,
            &["branch", "--list", "--format=%(refname:short)", &pattern],
        )
        .map_err(|source| Error::Git {
            op: "gate branch list",
            source,
        })?;
    for line in out.lines() {
        let branch = line.trim();
        if branch.is_empty() {
            continue;
        }
        let id = branch.strip_prefix("agents/").unwrap_or(branch);
        if role::derive(worktree, branch, id, git)?.as_deref() == Some(role)
            && is_ancestor(worktree, held_ref, branch, git)
        {
            return Ok(true);
        }
    }
    Ok(false)
}

#[cfg(test)]
mod tests;

/// `git merge-base --is-ancestor <a> <d>`: exit 0 (an `Ok`) iff `a` is an
/// ancestor of `d`. A non-zero exit (or a bad ref) is `Err` — read as "not
/// an ancestor", never propagated.
fn is_ancestor(worktree: &Path, ancestor: &str, descendant: &str, git: &dyn GitRunner) -> bool {
    git.run(
        worktree,
        &["merge-base", "--is-ancestor", ancestor, descendant],
    )
    .is_ok()
}

/// Remove a consumed inbox result message.
fn consume(cr: &ChildResult) -> Result<(), Error> {
    std::fs::remove_file(&cr.path).map_err(Error::Io)
}

/// The goal handed to `role` when it is dispatched at a child's return
/// (§6): the verifier's own boilerplate for the verifier — its soul
/// cannot know it judges a worker's completed work — and the generic
/// procedure goal plus the returning child's response for any other role.
/// The work is already in the child's inherited tree; the response is
/// quoted for convenience.
fn return_goal(
    role: &str,
    worktree: &Path,
    agent_id: &str,
    response: &str,
) -> Result<String, Error> {
    if role == VERIFIER_ROLE {
        return Ok(format!(
            "You are a verifier judging a worker's completed work — its output below and the \
             work products in your tree. Reply with a single leading verdict line: `APPROVE` if \
             the work satisfies the task, or `REJECT` followed by specific, actionable feedback.\n\n\
             Worker's final response:\n{response}\n"
        ));
    }
    let goal = procedure::goal(role, worktree, agent_id)?;
    Ok(format!(
        "{goal}\n\nThe returning child's final response:\n{response}\n"
    ))
}
