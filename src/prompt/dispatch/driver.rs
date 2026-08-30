//! The launched driver's own-branch entry (ARCH §2.11 exit protocol).
//!
//! Every launch — a writer's post-deposit probe, the `litany scan`
//! flush, an exiting executor's self-directed launch — spawns a driver
//! that runs this entry against its target agent. Warrant is decided
//! here, under the lock, never by the launcher (§2.11): [`drive`]
//! acquires-or-exits, and what it finds decides what happens.
//!
//! **The no-op driver path (§2.11 pin 1).** A driver that acquires and
//! finds nothing to deliver exits silently — no step, no epitaph — after
//! honouring the §2.11 **release rule** at its own lease release
//! ([`release_then_reprobe`], run by the `litany advance` hop): only a
//! deposit its own last inbox read never saw fires a launch, so a
//! found-nothing drive over a quiet inbox launches nothing and the
//! exit-launch recursion terminates here, while a deposit racing that
//! last read is no longer stranded (bl-9c8f). Found mail is
//! delivered through the ordinary step-boundary drain ([`super::drain`]
//! — delivery commits, work-product transfers included), after
//! rematerializing the worktree if quiescence tore it down (§2.3
//! step 6).
//!
//! **Scope note.** The step that *reacts* to delivered mail — "found-mail
//! → step to a new terminal → exit-launch again" (§2.11) — is `litany
//! advance`'s (§6, [`super::advance`]). This module is the own-branch
//! delivery entry that verb runs on arrival: `advance` holds its own
//! lease (adopted or acquired) and calls [`deliver`]; [`drive`] is the
//! acquire-and-deliver composition, the §2.11 contract in one call.

use super::{child_result, drain, transfer};
use crate::prompt::Error;
use crate::prompt::inbox;
use crate::template::GitRunner;
use crate::workspace;
use std::path::Path;

/// What one [`drive`] found and did — derived on the fly, nothing stored.
#[derive(Debug, PartialEq, Eq)]
#[cfg(test)] // `driver::drive` is a test-only entry; runtime delivers via `driver::deliver`.
pub enum DriveOutcome {
    /// Another executor holds the lock: the branch is already driven, so
    /// this driver exits as a clean no-op (§2.11 Writer/driver totality).
    AlreadyDriven,
    /// Acquired and found an empty inbox: the silent exit of §2.11
    /// pin 1 — no step, no epitaph, no further launch.
    NothingToDeliver,
    /// Acquired and delivered this many pending messages as delivery
    /// commits (§2.11 *Delivery*).
    Delivered(usize),
}

/// Drive `agent_id`'s branch: acquire-or-exit, then deliver whatever is
/// pending — or exit silently when nothing is (§2.11 pin 1). The lock is
/// held for the whole delivery and kernel-released on return.
#[cfg(test)] // test-only drive entry; runtime uses `deliver` (see DriveOutcome).
pub fn drive(workspace: &Path, agent_id: &str, git: &dyn GitRunner) -> Result<DriveOutcome, Error> {
    let inbox_dir = inbox::inbox_dir(workspace, agent_id);
    let Some(_lock) = inbox::try_acquire(&inbox_dir).map_err(|source| Error::ExecutorLock {
        path: inbox_dir.clone(),
        source,
    })?
    else {
        return Ok(DriveOutcome::AlreadyDriven);
    };
    let delivery = deliver(workspace, agent_id, git)?;
    match delivery.delivered + delivery.left.len() {
        0 => Ok(DriveOutcome::NothingToDeliver),
        n => Ok(DriveOutcome::Delivered(n)),
    }
}

/// Deliver `agent_id`'s pending mail under a lease the *caller* already
/// holds (§2.11 *Delivery* — only a lock-holding executor delivers):
/// rematerialize the worktree if quiescence tore it down, then run the
/// real drain (stray recovery + delivery commits). An empty inbox over a
/// torn-down worktree touches nothing; an empty inbox over a live
/// worktree still runs the drain's stray recovery, closing the §2.11
/// rename-without-commit crash window before the caller reads the tree.
///
/// Returns the drain's [`drain::Delivery`]: the delivery count, plus the
/// identities of the deposits deliberately left pending — the seen-set
/// [`release_then_reprobe`] diffs against once the lease is gone (§2.11
/// release rule).
pub(super) fn deliver(
    workspace: &Path,
    agent_id: &str,
    git: &dyn GitRunner,
) -> Result<drain::Delivery, Error> {
    let inbox_dir = inbox::inbox_dir(workspace, agent_id);
    let pending = drain::pending(&inbox_dir)?;
    let worktree = workspace::agent_worktree(workspace, agent_id);
    if !worktree.exists() {
        if pending.is_empty() {
            return Ok(drain::Delivery {
                delivered: 0,
                left: Vec::new(),
            });
        }
        rematerialize(workspace, agent_id, &worktree, git)?;
    }
    drain::drain(&worktree, &inbox_dir, agent_id, git)
}

/// The §2.11 **release rule** (the deposit rule's dual), as one funnel:
/// give up the lease, then re-read the inbox and — finding a deposit the
/// released executor's own last read never accounted for and whose own
/// warrant launches ([`deposit_warrants_launch`]) — complete that
/// deposit's launch through the unmodified writer seam
/// ([`inbox::probe_and_launch`]). A deposit racing the holder's last
/// inbox read meets a Busy probe (the writer defers to us), so the
/// launch it was owed becomes ours to make; the file our last read *did*
/// see and deliberately left (a gate-held result, §6) launches nothing,
/// so a hold never relaunch-loops. The diff runs on
/// [`drain::SeenDeposit`] file identities, never bare names: a delivered
/// or interpreted deposit frees its name for reuse, and a reused name is
/// a new deposit owed its launch like any other.
///
/// Ordering is the invariant: the re-read runs strictly *after* the
/// release, so a rival that took the freed lease first turns our probe
/// into the ordinary Busy deferral — no double-drive — and from the
/// release on this process only spawns and returns (§2.11 no-authority).
pub(super) fn release_then_reprobe(
    lock: inbox::ExecutorLock,
    workspace: &Path,
    agent_id: &str,
    seen: &[drain::SeenDeposit],
    launcher: &dyn inbox::Launcher,
) {
    drop(lock);
    reprobe_after_release(workspace, agent_id, seen, launcher);
}

/// The post-release half of [`release_then_reprobe`], split at the
/// release so the rival-holder deferral is exercisable deterministically
/// (a test acquires the lease, then runs this). Failures are logged and
/// swallowed — fire-and-forget, the §2.11 accepted crash class: the
/// stranding is late, never lost, and the next touch (a reprompt, a
/// hand-run `litany scan`) heals it.
pub(super) fn reprobe_after_release(
    workspace: &Path,
    agent_id: &str,
    seen: &[drain::SeenDeposit],
    launcher: &dyn inbox::Launcher,
) {
    let inbox_dir = inbox::inbox_dir(workspace, agent_id);
    let owed = match drain::pending(&inbox_dir) {
        Ok(pending) => pending
            .iter()
            .filter(|m| !seen.iter().any(|s| s.matches(m)))
            .any(|m| deposit_warrants_launch(&m.path)),
        Err(e) => {
            eprintln!(
                "litany: post-release inbox re-read for {agent_id}: {e} \
                 (accepted crash class, ARCH §2.11)"
            );
            return;
        }
    };
    if !owed {
        return;
    }
    if let Err(e) = inbox::probe_and_launch(workspace, agent_id, launcher) {
        eprintln!(
            "litany: post-release launch for {agent_id}: {e} (accepted crash class, ARCH §2.11)"
        );
    }
}

/// Whether an unseen racing deposit is *owed* a launch — the racing
/// writer's own launch decision, replayed (§2.11): the release rule
/// completes the launch the deposit would have gotten had it landed a
/// millisecond after the release, no more. A plain message's writer
/// (`litany message`, `litany dispatch`, the scan flush) always
/// launches; a result message's launch is pin 2's one epitaph decision —
/// `final-response` (the child's own `revive_parent`) and `died` (the
/// scan flush behind the sweep's deposit) wake the recipient, while
/// `stopped` and `budget-exhausted` deliberately park it: the §2.11
/// "stays undelivered, the next explicit touch delivers it" state,
/// identical raced or unraced — launching for those would erase the
/// parked state pin 2 specifies (deliver a kill report to the parent the
/// operator may be stopping next, or spam an exhausted ceiling). An
/// illegible deposit launches: no launcher decides warrant — the
/// launched driver does, under the lock (§2.11).
fn deposit_warrants_launch(path: &Path) -> bool {
    let Ok(body) = std::fs::read_to_string(path) else {
        return true;
    };
    if transfer::terminal_ref_of(&body).is_none() {
        return true;
    }
    let (epitaph, _) = child_result::split_frontmatter(&body);
    epitaph != inbox::Epitaph::Stopped.as_str()
        && epitaph != inbox::Epitaph::BudgetExhausted.as_str()
}

/// Rematerialize a torn-down quiescent worktree off the persistent
/// branch ref (§2.3 step 6 — the worktree is disposable materialization,
/// never state): `git worktree add <path> agents/<id>`, run against the
/// workspace's bare `repo.git` (§2.2).
fn rematerialize(
    workspace: &Path,
    agent_id: &str,
    worktree: &Path,
    git: &dyn GitRunner,
) -> Result<(), Error> {
    let wt_str = worktree.to_string_lossy().to_string();
    let branch_ref = workspace::agent_ref(agent_id);
    git.run(
        &workspace::repo_git(workspace),
        &["worktree", "add", wt_str.as_str(), branch_ref.as_str()],
    )
    .map_err(|source| Error::Git {
        op: "worktree add (rematerialize)",
        source,
    })
}

#[cfg(test)]
mod release_tests;
#[cfg(test)]
mod tests;
