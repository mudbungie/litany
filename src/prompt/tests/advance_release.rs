//! The §2.11 release rule at the `litany advance` hop (bl-9c8f): the
//! lost-wakeup race made deterministic *inside* `dispatch::advance::run`.
//! The injected resolve is the seat: it runs strictly after the drain's
//! inbox enumeration (the hop's last read) and strictly before the
//! release — depositing there is the race, with no sleeps and no load.
//! A gate-held result opens that seat on the no-op exits; a stop flag
//! opens it on the terminal arm (the pin-2 interaction).

use super::advance::{AGENT, RecLauncher, terminal_tail, worker_config, workspace_with_tail};
use super::fixtures::*;
use crate::config::Workflow;
use crate::prompt::dispatch::advance::{AdvanceOutcome, run};
use crate::prompt::inbox::{self, Epitaph, inbox_dir};
use crate::prompt::resolve::WorkerConfig;
use std::sync::atomic::AtomicBool;

/// A child of [`AGENT`] (its id plus one descent segment, §2.3).
const CHILD: &str = "20260101-a1-20260102-b2";

/// A workflow that holds a returning worker's result in the inbox — the
/// §6 gate, the one legal way mail the hop *saw* stays pending.
fn gate_config() -> WorkerConfig {
    let mut cfg = worker_config();
    cfg.workflow = Workflow::parse(
        "events:\n  worker_return:\n    - gate_return_on(verifier.approve)\n",
        std::path::Path::new("workflow.yaml"),
    )
    .unwrap();
    cfg
}

/// A hop over a terminal tail with one gate-held child result pending.
/// `race` additionally deposits a user message from inside the resolve
/// seat — after the hop's last inbox read, before its release.
fn no_op_hop_with_held_result(race: bool) -> (tempfile::TempDir, RecLauncher, AdvanceOutcome) {
    let (ws, _wt) = workspace_with_tail(&terminal_tail());
    let (clock, id) = (FixedClock::default(), FixedIdGen);
    inbox::deposit_result(
        ws.path(),
        AGENT,
        CHILD,
        Epitaph::FinalResponse,
        "abc123",
        None,
        &clock,
        &StubGit::ok(),
    )
    .unwrap();
    let (adapter, sleeper, git) = (unreachable_adapter(), StubSleeper::default(), StubGit::ok());
    let tools = StubToolExecutor::ok();
    let rec = RecLauncher::default();
    let mut deps = valid_deps(&adapter, &sleeper, &git, &clock, &id, &tools, ws.path());
    deps.launcher = &rec;
    let out = run(ws.path(), AGENT, None, &deps, &mut || {
        if race {
            inbox::deposit(ws.path(), AGENT, "user", "racing mail", &clock).unwrap();
        }
        Ok(gate_config())
    })
    .unwrap();
    (ws, rec, out)
}

#[test]
fn a_deposit_racing_the_hops_last_read_is_launched_at_its_release() {
    // Without the release rule this deposit was stranded forever: its
    // writer's probe read Busy (the hop held the lease), and the hop's
    // pin-1 exit launched nothing (bl-9c8f). Now the no-op release
    // re-reads and completes the deposit's launch at its own agent.
    let (ws, rec, out) = no_op_hop_with_held_result(true);
    assert!(matches!(out, AdvanceOutcome::NothingToDo));
    assert_eq!(*rec.invocations.borrow(), vec![AGENT.to_string()]);
    // Launching is not delivering: both files still await the launched
    // driver — the held result by the gate, the racing mail by delivery.
    assert_eq!(
        std::fs::read_dir(inbox_dir(ws.path(), AGENT))
            .unwrap()
            .count(),
        2
    );
}

#[test]
fn a_deposit_racing_a_stopped_terminal_release_is_launched_whatever_the_epitaph() {
    // The §2.11 pin-2 interaction, as ruled ("Terminal releases run the
    // same rule"): pin 2 denies the epitaph-funded launches — a stopped
    // branch relaunches nothing and wakes no parent — but a deposit that
    // raced the hop's last inbox read is new work (§2.9's resume path),
    // and the terminal tail's release funnel launches for it.
    let (ws, _wt) = workspace_with_tail(&terminal_tail());
    let (clock, id) = (FixedClock::default(), FixedIdGen);
    // Delivered pre-mail turns the tail user-side, so the hop reaches
    // the step whose entry stop-check goes Terminal(Stopped).
    inbox::deposit(ws.path(), AGENT, "user", "work", &clock).unwrap();
    let (adapter, sleeper, git) = (unreachable_adapter(), StubSleeper::default(), StubGit::ok());
    let tools = StubToolExecutor::ok();
    let rec = RecLauncher::default();
    let stopped = AtomicBool::new(true);
    let mut deps = valid_deps(&adapter, &sleeper, &git, &clock, &id, &tools, ws.path());
    deps.launcher = &rec;
    deps.stop = &stopped;
    let out = run(ws.path(), AGENT, None, &deps, &mut || {
        // The race seat: after the hop's last inbox read, before release.
        inbox::deposit(ws.path(), AGENT, "user", "racing mail", &clock).unwrap();
        Ok(worker_config())
    })
    .unwrap();
    assert!(matches!(out, AdvanceOutcome::Terminal));
    // Exactly one launch: the release rule's, at own agent — the stopped
    // epitaph funded none (the no-race twin is advance_edges').
    assert_eq!(*rec.invocations.borrow(), vec![AGENT.to_string()]);
    // Launching is not delivering: the racing mail awaits its driver.
    assert_eq!(
        std::fs::read_dir(inbox_dir(ws.path(), AGENT))
            .unwrap()
            .count(),
        1
    );
}

#[test]
fn a_held_result_alone_never_relaunches_the_no_op_hop() {
    // Termination: the held result was in the hop's last read (the
    // drain enumerated and left it), so the release fires nothing —
    // pin 1 stays silent and a gate-hold cannot relaunch-loop.
    let (ws, rec, out) = no_op_hop_with_held_result(false);
    assert!(matches!(out, AdvanceOutcome::NothingToDo));
    assert!(rec.invocations.borrow().is_empty());
    assert_eq!(
        std::fs::read_dir(inbox_dir(ws.path(), AGENT))
            .unwrap()
            .count(),
        1
    );
}
