//! `litany advance <workspace> <agent>` — the §6 driver verb: one hop
//! of the workflow chain.
//!
//! Every §2.11 launch seam spawns this verb (detached, via the injected
//! launcher), the exec baton hands one hop to the next (§6), and an
//! operator runs the same verb by hand — launch after a crash, launch
//! after a deposit, and hand-run are indistinguishable, which is the
//! §6 collapse of "advance" and "resume". A hop:
//!
//! 1. **takes the lease** — an adopted predecessor fd or a fresh
//!    acquire ([`crate::prompt::inbox::baton`]); losing the acquire is
//!    the clean no-op of Writer/driver totality (§2.11).
//! 2. **delivers** — [`super::driver::deliver`]: rematerialize, stray
//!    recovery, delivery commits (§2.11).
//! 3. **derives warrant from the tree** ([`warrant`]) — no launcher
//!    decides warrant; the driver decides under the lock (§2.11). The
//!    derivation is the wire alternation itself (§2.3): a transcript
//!    tail ending user-side means a model call is due.
//! 4. **runs one step** ([`hop`]) — the same step machinery
//!    [`super::run_exchange`] drives.
//! 5. **hands off** — tools ran → [`AdvanceOutcome::ToolsPending`]
//!    carries the live lease out for the caller to exec the successor
//!    (§6 exec baton, [`cli`]); a terminal event ends the chain through
//!    the §2.11 exit protocol (deposit at the terminal event → release →
//!    epitaph-valued launches, at own agent and at the parent the
//!    deposit revived → return).
//!
//! Config resolution is **lazy** (the `resolve` closure): a no-op hop
//! exits before any config file is read or any `bz --version` guard
//! runs, so the pin-1 recursion terminator costs nothing but the probe.

pub mod cli;
mod crash;
mod held;
mod hop;

#[cfg(test)]
mod tests;

use super::{assembler, child_result, driver, terminal};
use crate::prompt::inbox::{self, ExecutorLock};
use crate::prompt::notice::notice;
use crate::prompt::resolve::WorkerConfig;
use crate::prompt::{Deps, Error, retarget};
use crate::workspace::hold;
use brazen::{Content, Message, Role};
use std::path::Path;

/// What one hop found and did — derived on the fly, nothing stored.
#[derive(Debug)]
pub enum AdvanceOutcome {
    /// Another executor holds the lock: the branch is already driven —
    /// the clean no-op of Writer/driver totality (§2.11).
    AlreadyDriven,
    /// Nothing is due: empty inbox and a transcript tail with no model
    /// call pending — the §2.11 pin-1 silent exit, terminating the
    /// exit-launch recursion. The exit honours the §2.11 release rule
    /// ([`driver::release_then_reprobe`]): a deposit that raced this
    /// hop's last inbox read is launched for after the release, so
    /// "silent" never means "stranding".
    NothingToDo,
    /// The hop stepped to a terminal event and ran the §2.11 exit
    /// protocol; the chain ends here. The epitaph is not carried — the
    /// exit protocol already wrote it to disk (the deposited result
    /// message, §2.11), its single authoritative home (PRINCIPLES SSOT).
    Terminal,
    /// The step emitted `tool_use` and its tools ran: the successor hop
    /// must run. Carries the live lease for the §6 exec baton — the
    /// caller preps the successor command ([`cli`], `baton`) and execs.
    ToolsPending(ExecutorLock),
    /// The configured control held an invocation (§3.3 *Tool control*):
    /// the branch is parked mid-tool-window — no terminal, no deposit,
    /// the lease released. The whole state is disk-derived: the hold
    /// mark ([`crate::workspace::hold`]) plus the unpaired tail. A later
    /// drive of the same agent re-adjudicates ([`held`]).
    Held,
}

/// What the transcript tail warrants (§6 hop step 3).
#[derive(Debug, PartialEq, Eq)]
enum Warrant {
    /// Tail ends user-side (delivered mail, committed tool results): a
    /// model call is due.
    ModelCallDue,
    /// Tail ends assistant-side without `tool_use`, or is empty: nothing
    /// is due (§2.11 pin 1).
    NothingDue,
    /// A `tool_use` unmatched by any committed `tool_result` — wherever
    /// it sits: the one non-replayable state (§6), declined loudly.
    Unpaired,
}

/// Derive warrant from the assembled wire history (§6). The §2.3
/// pairing invariant is judged over the **whole alternation**, not the
/// tail: a `tool_use` with no committed `tool_result` is [`Warrant::Unpaired`]
/// even with delivered mail behind it — the tail's role cannot answer
/// for it, because `driver::deliver` runs before this derivation, so a
/// crash-orphaned window is routinely *buried* user-side by the time it
/// is read (bl-15f0; sent anyway, the provider rejects the history
/// forever). Only a fully paired history lets the tail speak: user- or
/// tool-side means a model call is due (committed tool results compose
/// tool-side, canonical `Role::Tool`, §2.3, and delivered mail
/// user-side — the same observation), assistant-side or empty means
/// nothing is due.
fn warrant(messages: &[Message]) -> Warrant {
    let mut unanswered = std::collections::HashSet::new();
    for m in messages {
        for b in &m.content {
            match b {
                Content::ToolUse { id, .. } => {
                    unanswered.insert(id.as_str());
                }
                Content::ToolResult { tool_use_id, .. } => {
                    unanswered.remove(tool_use_id.as_str());
                }
                _ => {}
            }
        }
    }
    if !unanswered.is_empty() {
        return Warrant::Unpaired;
    }
    match messages.last() {
        Some(m) if matches!(m.role, Role::User | Role::Tool) => Warrant::ModelCallDue,
        _ => Warrant::NothingDue,
    }
}

/// Report what consuming a retarget mark did (§2.2). `None` — no mark —
/// is every boundary but one and says nothing. Of the rest only the
/// **decline** speaks, the same rule the compaction landing follows: a
/// landing that did what it said needs no line, and the operator's
/// confirmation was the verb's own (§3.4). A decline is the one outcome
/// nothing else surfaces.
fn report_retarget(agent_id: &str, outcome: Option<retarget::Outcome>) {
    if let Some(retarget::Outcome::Conflicted(paths)) = outcome {
        notice!(
            "retarget of [{agent_id}] declined — git could not replay {} \
             (marked refs/litany/conflicted/{agent_id}, ARCH §2.6); the branch continues \
             on its previous config",
            paths.join(", "),
        );
    }
}

/// Run one hop against `agent_id`'s branch. `lease` is a lease the
/// caller already took (the adopted §6 baton fd); `None` acquires here
/// — losing the acquire is [`AdvanceOutcome::AlreadyDriven`]. `resolve`
/// loads the role config lazily, only once a step is warranted (`&mut
/// dyn` rather than `impl FnOnce` so the function has one instantiation
/// and one coverage record).
pub(in crate::prompt) fn run(
    workspace: &Path,
    agent_id: &str,
    lease: Option<ExecutorLock>,
    deps: &Deps<'_>,
    resolve: &mut dyn FnMut() -> Result<WorkerConfig, Error>,
) -> Result<AdvanceOutcome, Error> {
    let lock = match lease {
        Some(lock) => lock,
        None => {
            let inbox_dir = inbox::inbox_dir(workspace, agent_id);
            match inbox::try_acquire(&inbox_dir).map_err(|source| Error::ExecutorLock {
                path: inbox_dir.clone(),
                source,
            })? {
                Some(lock) => lock,
                None => return Ok(AdvanceOutcome::AlreadyDriven),
            }
        }
    };

    // A hold mark parks the branch mid-tool-window (§3.3 *Tool
    // control*), and the held entry runs **before delivery** — mail
    // delivered onto an unpaired tail would wedge between a `tool_use`
    // and its `tool_result` (§2.3 pairing), so a parked branch queues
    // its mail instead ([`held`]). A stale mark is cleared there and the
    // ordinary hop continues below.
    let lock = match hold::read(workspace, agent_id, deps.git) {
        Some(mark) => match held::resume(workspace, agent_id, &mark, lock, deps, resolve)? {
            held::Resumption::Done(outcome) => return Ok(outcome),
            held::Resumption::Stale(lock) => lock,
        },
        None => lock,
    };

    // §6 crash settlement (bl-4187), strictly before delivery so the
    // settlement lands ahead of any mail ([`crash`]).
    crash::settle_crashed_window(workspace, agent_id, deps)?;

    // `delivery.left` is what this executor's last inbox read under the
    // lease deliberately left pending — the §2.11 release rule's diff
    // base for every voluntary release below (the two no-op exits and
    // the terminal arm alike).
    let delivery = driver::deliver(workspace, agent_id, deps.git)?;
    let seen = delivery.left;
    let worktree = crate::workspace::agent_worktree(workspace, agent_id);
    if !worktree.exists() {
        // Torn down and no mail: quiescent, nothing due (§2.3 step 6).
        driver::release_then_reprobe(lock, workspace, agent_id, &seen, deps.launcher);
        return Ok(AdvanceOutcome::NothingToDo);
    }

    // §2.2 *Fork is the freeze*, and its one exit: a retarget mark
    // (`litany retarget`, §3.4) lands **here** — at the step boundary,
    // before anything resolves config — so the step below is the first one
    // the target config governs. Unmarked, which is every agent at every
    // boundary bar one, costs a single ref read.
    report_retarget(
        agent_id,
        retarget::land(workspace, agent_id, &worktree, deps.git)?,
    );

    // §6 delivered-child-result circumstance: interpret any result message
    // the drain left in the inbox (deliver_result / land_compaction / a
    // gate-hold, keyed on the returning child's role). This needs the
    // workflow, so resolve once when a result is pending — a no-op hop has
    // none and still resolves nothing (lazy resolution). The resolved
    // config is reused by the step below rather than read twice.
    let mut cfg = None;
    if child_result::has_pending_result(workspace, agent_id)? {
        let resolved = resolve()?;
        child_result::interpret_pending(workspace, agent_id, &worktree, &resolved.workflow, deps)?;
        cfg = Some(resolved);
    }

    // Warrant derives from the transcript tail alone (§2.3, §6): the
    // §5.2 head/body sits ahead of the tail and must not read as
    // user-side mail warranting a model call — and the transcript-only
    // composition keeps a no-op hop config-free (lazy resolution,
    // above).
    match warrant(&assembler::transcript(&worktree)?) {
        Warrant::NothingDue => {
            // §2.11 pin 1, closed by the release rule: the silent exit
            // is silent only over an inbox this hop's own last read
            // fully accounted for — a deposit that raced that read met a
            // Busy writer probe and is owed its launch by us.
            driver::release_then_reprobe(lock, workspace, agent_id, &seen, deps.launcher);
            Ok(AdvanceOutcome::NothingToDo)
        }
        Warrant::Unpaired => Err(Error::UnpairedToolUse {
            branch: agent_id.to_string(),
        }),
        Warrant::ModelCallDue => {
            let cfg = match cfg {
                Some(cfg) => cfg,
                None => resolve()?,
            };
            match hop::step(workspace, agent_id, &worktree, &cfg, deps)? {
                hop::StepOutcome::ToolsRan => Ok(AdvanceOutcome::ToolsPending(lock)),
                hop::StepOutcome::Held => {
                    // Fresh park (§3.3 *Tool control*): the seam wrote
                    // the mark; release through the release rule and
                    // exit without a terminal.
                    driver::release_then_reprobe(lock, workspace, agent_id, &seen, deps.launcher);
                    Ok(AdvanceOutcome::Held)
                }
                hop::StepOutcome::Terminal(epitaph) => {
                    // The shared §2.11 terminal tail ([`terminal::conclude`]
                    // — the same sequence as `run_exchange`'s): finish by
                    // epitaph value, terminal-lifecycle bindings (§6),
                    // release through the release rule (a racing deposit
                    // launches whatever the epitaph), then the
                    // epitaph-valued exit launches. No terminal compaction
                    // (§2.7 — the stage is deleted).
                    terminal::conclude(
                        workspace,
                        agent_id,
                        epitaph,
                        &cfg.workflow,
                        lock,
                        &seen,
                        deps,
                    )?;
                    Ok(AdvanceOutcome::Terminal)
                }
            }
        }
    }
}
