//! What the step loop does on its way out (ARCH §2.9, §2.11, §6).
//!
//! Three terminal shapes reach [`finish`], keyed by epitaph value (§2.6
//! — code branches on the value, never on shape): a `stopped` branch
//! (§2.9), an exhausted branch (§6), and the ordinary final-response
//! branch. Only a `stopped` branch has an outstanding deposit to make
//! here; the final response deposited in the loop and the exhausted
//! branch at the boundary check.
//!
//! **There is no terminal compaction** (§2.7: "There is no terminal
//! compaction stage anymore"). The v0.3 compactor dispatch that fired at
//! every final response is deleted: a child's result message carries its
//! own terminal response (§2.6), not a compactor product, and with
//! merge-back gone there is no merge payload to slim before returning.
//! Compaction now runs only at configured checkpoints during a branch's
//! life ([`crate::prompt::compactor::checkpoint`]).
//!
//! The stopped deposit is the §2.9 step-3 return performed *outside* the
//! signal handler ([`super::stop_signal`]) — the executor's SIGTERM
//! handler set a flag, the loop broke at a check point, and this is the
//! final deposit before the process exits. It reads the branch tip as the
//! terminal ref and deposits a `stopped`-epitaph result with no body (a
//! stopped agent has almost never finished speaking; [`deposit_result`]
//! renders a body-absent message either way). A stop is an **obituary**
//! (§2.6), so it is addressed to the dispatcher whoever prompted the
//! agent last, and a root has no dispatcher — the deposit is a
//! structural no-op there ([`super::result_deposit::recipient`]). The
//! stop *signature*
//! — the missing trailing `end` on the branch's own `response.json` —
//! lives on a different tree and is untouched by this deposit.
//!
//! [`conclude`] is the whole terminal tail, shared by both drivers. Its
//! lease release runs the §2.11 **release rule**
//! ([`super::driver::release_then_reprobe`]): a deposit that raced the
//! executor's last inbox read is launched for whatever the epitaph — it
//! is new work, and §2.9 makes messaging a stopped branch the resume
//! path. [`exit_launch`] then closes the §2.11 exit protocol with the
//! epitaph-*funded* launches: a driver spawned at the exiting agent
//! itself, fire-and-forget, and — following the result deposit that just
//! landed — a driver at the **recipient** ([`revive_recipient`]). Both
//! are decided by one epitaph value (§2.11 pin 2): a final response
//! launches; `stopped` never does (a relaunch funded by nothing new
//! would resurrect the branch the operator just killed, and waking the
//! dispatcher would hand it a stop to undo one level up);
//! `budget-exhausted` never does (an epitaph-spam cycle against a hard
//! ceiling — one the dispatcher shares, since the ceiling is derived
//! over the whole tree, §6).
//!
//! **Two launches, one sequence.** §2.11's terminal sequence — deposit
//! the result message → release own lock → spawn a driver at own agent →
//! exit — names only the self-directed launch, because the recipient-side
//! one is not the exit protocol's: it is the *deposit's*, the same "a
//! deposit into a quiescent agent starts a driver" rule `litany message`
//! obeys (Writer/driver totality — a writer deposits, probes, and
//! launches). The terminal deposit is that writer act, so it rides the
//! same seam ([`crate::prompt::inbox::probe_and_launch`], not a second
//! copy of the probe/spawn logic) and it runs *after* the exiting
//! executor releases its own lock: from then on the exiting process has
//! no authority over its own branch, and a revived recipient that
//! immediately messages or stops this agent meets no lingering lease.
//! Deposit and wake address the same [`super::result_deposit::recipient`]
//! — the one home of the addressing rule — so they cannot disagree about
//! who the message was for.
//!
//! [`deposit_result`]: crate::prompt::inbox::deposit_result

use super::super::budget;
use super::super::inbox::{self, Epitaph};
use super::super::{Deps, Error};
use super::result_deposit::{self, deposit_terminal};
use crate::config::{Budgets, Workflow};
use crate::prompt::notice::notice;
use std::path::Path;

/// The whole §2.11 terminal tail — one sequence for both drivers
/// ([`super::run_exchange`]'s tail and the `litany advance` hop), so the
/// two terminal lease releases are literally one code path: finish by
/// epitaph value ([`finish`]), evaluate the workflow's terminal-lifecycle
/// bindings (§6 — the epitaph names the event), release the lease through
/// the §2.11 **release rule** ([`super::driver::release_then_reprobe`] —
/// a deposit that raced this executor's last inbox read, `seen`, is
/// launched for *regardless of the epitaph*: it is new work, and §2.9
/// makes messaging a stopped branch the resume path), then the
/// epitaph-valued launches at own agent and at the recipient the result
/// deposit revived ([`exit_launch`]). After the release this process has
/// no authority: spawn and return are its only acts.
pub(super) fn conclude(
    workspace: &Path,
    agent_id: &str,
    epitaph: Epitaph,
    workflow: &Workflow,
    lock: inbox::ExecutorLock,
    seen: &[super::drain::SeenDeposit],
    deps: &Deps<'_>,
) -> Result<(), Error> {
    let worktree = crate::workspace::agent_worktree(workspace, agent_id);
    finish(workspace, agent_id, &worktree, epitaph, deps)?;
    // The recipient the deposit addressed, re-derived from the same
    // transcript under the still-held lock — the rule's one home is
    // [`result_deposit::recipient`], and asking it here rather than
    // carrying a copy keeps the address derived (PRINCIPLES SSOT).
    // *Before* the release: from the release on, a rival executor may
    // deliver new mail and move the tail, and the wake must address the
    // inbox the deposit actually landed in.
    let recipient = result_deposit::recipient(&worktree, agent_id, epitaph)?;
    crate::prompt::workflow_actions::run_terminal_bindings(
        workflow, epitaph, &worktree, agent_id, deps.git,
    )?;
    super::driver::release_then_reprobe(lock, workspace, agent_id, seen, deps.launcher);
    exit_launch(workspace, agent_id, epitaph, recipient.as_deref(), deps);
    Ok(())
}

/// The §6 budget check at a model-call boundary: tokens/wall/depth derived
/// live over the tree (no stored counter, PRINCIPLES SSOT). On exhaustion
/// it writes `refs/litany/budget-exhausted/<branch>`, deposits a
/// `budget-exhausted` result (the agent did not speak this step, so no
/// body), and returns `true` so the loop ceases — an ordinary terminal
/// state (§2.9). `false` continues the loop.
pub(super) fn budget_exhausted(
    repo: &Path,
    conv_id: &str,
    branch: &str,
    worktree: &Path,
    budgets: &Budgets,
    deps: &Deps<'_>,
) -> Result<bool, Error> {
    let Some(ex) = budget::check(repo, branch, budgets) else {
        return Ok(false);
    };
    notice!("budget {ex} on {branch}; stopping (ARCH §6)");
    budget::mark_exhausted(worktree, branch, deps.git).map_err(|source| Error::Git {
        op: "budget-exhausted update-ref",
        source,
    })?;
    deposit_terminal(
        repo,
        conv_id,
        worktree,
        Epitaph::BudgetExhausted,
        None,
        deps,
    )?;
    Ok(true)
}

/// Finish the exchange by epitaph value (§2.6). Only `stopped` has an
/// outstanding deposit here — its result is deposited on the way out
/// (§2.9 step 3). A final response deposited inside the loop and a
/// `budget-exhausted` branch at the boundary check, so both are no-ops.
/// No terminal compaction is dispatched (§2.7 — the stage is deleted).
fn finish(
    repo: &Path,
    conv_id: &str,
    worktree: &Path,
    epitaph: Epitaph,
    deps: &Deps<'_>,
) -> Result<(), Error> {
    if epitaph == Epitaph::Stopped {
        deposit_terminal(repo, conv_id, worktree, Epitaph::Stopped, None, deps)
    } else {
        Ok(())
    }
}

/// The launches closing the §2.11 exit protocol, called *after* the
/// executor lock is released: a driver at this agent (the self-directed
/// launch) and a driver at the `recipient` the result deposit just landed
/// in ([`revive_recipient`]), both fire-and-forget and both by epitaph
/// value (§2.11 pin 2). These are the epitaph-*funded* launches — the
/// launch a racing deposit funds is the release rule's, made in
/// [`conclude`] before this runs, whatever the epitaph. Fire-and-forget
/// is literal — a launch failure is logged and swallowed, never
/// propagated: it falls into the accepted crash class (§2.11), where the
/// stranding is late, not lost, and the next touch (a reprompt, or a
/// hand-run `litany scan`) heals it.
fn exit_launch(
    workspace: &Path,
    agent_id: &str,
    epitaph: Epitaph,
    recipient: Option<&str>,
    deps: &Deps<'_>,
) {
    // §2.11 pin 2: only a final response launches — stopped and
    // budget-exhausted never relaunch. (`died` never reaches an exit
    // path at all: a dead executor runs nothing.)
    if epitaph != Epitaph::FinalResponse {
        return;
    }
    if let Err(e) = deps.launcher.launch(workspace, agent_id) {
        notice!("exit launch for {agent_id}: {e} (accepted crash class, ARCH §2.11)");
    }
    revive_recipient(workspace, recipient, deps);
}

/// Start a driver at the agent whose inbox this agent's result message
/// just landed in (§2.11 "a deposit into a quiescent agent starts a
/// driver" — revival-on-deposit, §2.5). The address is the *same*
/// [`result_deposit::recipient`] the deposit derived — the deposit and
/// the wake-up cannot disagree about who the message was for — so a
/// terminal that addressed nobody (an operator-prompted reply, a root's
/// obituary) skips it exactly as the deposit did.
///
/// This is the writer's post-deposit probe, unmodified and unduplicated:
/// [`inbox::probe_and_launch`] — the seam `litany message` runs — so a
/// recipient whose lease is held gets nothing (its own executor drains
/// at its next boundary, §2.11 Delivery) and a quiescent one gets exactly
/// one detached `litany advance`, whose warrant is decided under the
/// lock like any other driver's.
fn revive_recipient(workspace: &Path, recipient: Option<&str>, deps: &Deps<'_>) {
    let Some(recipient) = recipient else {
        return;
    };
    if let Err(e) = inbox::probe_and_launch(workspace, recipient, deps.launcher) {
        notice!("revival launch for {recipient}: {e} (accepted crash class, ARCH §2.11)");
    }
}
