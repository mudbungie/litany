//! The v0.7 success criterion (ARCH §6): a **verifier gating a worker's
//! return, config-only, end-to-end**, on the stub-adapter harness with
//! real git. The gate is expressed entirely as `workflow.yaml` bindings
//! (`worker_return: [dispatch(verifier), gate_return_on(verifier.approve)]` +
//! `verifier_approve: [deliver_result]`) plus a `souls/verifier.md`
//! authored into the config — no code path is special-cased to it.
//!
//! The flow is driven through the ordinary `litany advance` hop three
//! times, each a distinct agent: (1) the gating parent interprets the
//! worker's return and dispatches the verifier off the worker's terminal
//! ref, holding delivery; (2) the verifier steps and its stub model
//! emits an `APPROVE` verdict, returning it to the parent; (3) the parent
//! interprets the verdict, drains the held worker result, and steps to
//! react. Every state is disk-derived — the hold is "worker result in the
//! inbox + verifier dispatched + not yet approved", never a flag.

use super::advance::{RecLauncher, worker_config};
use super::fixtures::*;
use crate::config::Workflow;
use crate::prompt::dispatch::advance::{AdvanceOutcome, run};
use crate::prompt::inbox::{self, deposit_result};
use crate::prompt::resolve::WorkerConfig;
use crate::prompt::{Deps, IdGen, SystemClock};
use crate::template::{GitRunner, RealGit};
use crate::workspace::agent_name::mint::test_rng;
use crate::workspace::{agent_worktree, fixture};
use brazen::FinishReason;
use std::cell::Cell;
use std::path::Path;
use std::sync::atomic::AtomicBool;

/// A sequence [`IdGen`] so the parent's two dispatches (worker, then
/// verifier) get distinct hyphen-free sub-ids in one workspace.
struct SeqIdGen(Cell<u32>);
impl IdGen for SeqIdGen {
    fn short(&self) -> String {
        let n = self.0.get();
        self.0.set(n + 1);
        format!("id{n:04}")
    }
}

const GATE_WORKFLOW: &str = "events:\n  \
    worker_return:\n    - dispatch(verifier)\n    - gate_return_on(verifier.approve)\n  \
    verifier_approve:\n    - deliver_result\n";

fn gate_cfg(role: &str, workflow: &str) -> WorkerConfig {
    WorkerConfig {
        role: role.into(),
        workflow: Workflow::parse(workflow, Path::new("workflow.yaml")).unwrap(),
        ..worker_config()
    }
}

/// Build a [`Deps`] borrowing the shared components, varying only the
/// adapter per hop.
#[allow(clippy::too_many_arguments)]
fn deps<'a>(
    adapter: &'a StubAdapter,
    sleeper: &'a StubSleeper,
    git: &'a RealGit,
    clock: &'a SystemClock,
    id: &'a SeqIdGen,
    tools: &'a StubToolExecutor,
    stop: &'a AtomicBool,
    launcher: &'a RecLauncher,
    cfg_root: &'a Path,
) -> Deps<'a> {
    Deps {
        adapter,
        sleeper,
        git,
        clock,
        id_gen: id,
        tool_executor: tools,
        config_root: cfg_root,
        adapter_target: None,
        stop,
        launcher,
        rng: test_rng(),
    }
}

#[test]
fn a_verifier_gates_a_workers_return_end_to_end_config_only() {
    let (_h, ws) = fixture::workspace();
    // Config-only: author a verifier soul into the config the parent forks
    // off (child_dispatch reads `souls/verifier.md` from the governing
    // config commit). The gate itself is the workflow bindings below.
    fixture::amend_config(&ws, &[("souls/verifier.md", "You are a verifier.")]);
    let parent = "20260101-a1";
    let parent_wt = fixture::spawn_root(&ws, parent);

    let git = RealGit::new();
    let clock = SystemClock;
    let id = SeqIdGen(Cell::new(1));
    let tools = StubToolExecutor::ok();
    let sleeper = StubSleeper::default();
    let stop = AtomicBool::new(false);
    let rec = RecLauncher::default();
    let cfg_root = tempfile::TempDir::new().unwrap();

    // A worker child returned: fork it, commit a work product, deposit its
    // result into the parent's inbox (epitaph final-response).
    use crate::prompt::child_dispatch::{ChildDispatchRequest, run as dispatch_child};
    let wreq = ChildDispatchRequest {
        repo: &ws,
        parent_branch: parent,
        parent_worktree: &parent_wt,
        role: "worker",
        goal: "do it",
        name: None,
        fork_point: None,
        cwd: None,
        pins: crate::prompt::PinnedDocs::none(),
    };
    let worker = dispatch_child(&wreq, &git, &clock, &id, &rec, test_rng()).unwrap();
    let worker_wt = agent_worktree(&ws, &worker);
    std::fs::write(worker_wt.join("out.txt"), "the work\n").unwrap();
    git.run(&worker_wt, &["add", "-A"]).unwrap();
    git.run(&worker_wt, &["commit", "-m", "work"]).unwrap();
    let wtip = git.run_capture(&worker_wt, &["rev-parse", "HEAD"]).unwrap();
    deposit_result(
        &ws,
        parent,
        &worker,
        inbox::Epitaph::FinalResponse,
        wtip.trim(),
        Some("done"),
        &clock,
        &git,
    )
    .unwrap();

    // (1) Parent hop: interpret the worker return under the gate → dispatch
    // a verifier off the worker's terminal ref, hold the worker result.
    let hold = unreachable_adapter();
    let d1 = deps(
        &hold,
        &sleeper,
        &git,
        &clock,
        &id,
        &tools,
        &stop,
        &rec,
        cfg_root.path(),
    );
    let out = run(&ws, parent, None, &d1, &mut || {
        Ok(gate_cfg("worker", GATE_WORKFLOW))
    })
    .unwrap();
    assert!(
        matches!(out, AdvanceOutcome::NothingToDo),
        "held, no parent step: {out:?}"
    );
    // The worker result is still held in the inbox (undelivered).
    assert!(
        inbox::inbox_dir(&ws, parent)
            .join(format!("{worker}-001.md"))
            .exists()
    );
    // A verifier child was launched off the worker's terminal ref.
    let verifier = rec
        .invocations
        .borrow()
        .iter()
        .find(|a| **a != worker)
        .cloned()
        .expect("verifier launched");
    assert!(
        git.run(
            &parent_wt,
            &[
                "merge-base",
                "--is-ancestor",
                wtip.trim(),
                &crate::workspace::agent_ref(&verifier)
            ]
        )
        .is_ok()
    );

    // (2) Verifier hop: it steps and its stub model emits an APPROVE
    // verdict, returning it to the parent's inbox.
    let approve = StubAdapter::scripted([StubAdapter::reply_ok(&stream_of(
        FinishReason::Stop,
        &[Block::Text("APPROVE")],
    ))]);
    let d2 = deps(
        &approve,
        &sleeper,
        &git,
        &clock,
        &id,
        &tools,
        &stop,
        &rec,
        cfg_root.path(),
    );
    let out = run(&ws, &verifier, None, &d2, &mut || {
        Ok(gate_cfg("verifier", "events: {}\n"))
    })
    .unwrap();
    assert!(matches!(out, AdvanceOutcome::Terminal), "{out:?}");
    // The verifier is a child, so its terminal epitaph IS deposited to
    // disk (the parent's inbox) — the authoritative record the outcome no
    // longer mirrors. Assert the deposited result names the final-response
    // epitaph.
    let deposited =
        std::fs::read_to_string(inbox::inbox_dir(&ws, parent).join(format!("{verifier}-001.md")))
            .unwrap();
    assert!(
        deposited.contains("epitaph: final-response"),
        "{deposited:?}"
    );

    // (3) Parent hop: interpret the verdict → deliver the held worker
    // result → step to react (stub final response).
    let react = StubAdapter::scripted([StubAdapter::reply_ok(&happy_response_bytes())]);
    let d3 = deps(
        &react,
        &sleeper,
        &git,
        &clock,
        &id,
        &tools,
        &stop,
        &rec,
        cfg_root.path(),
    );
    let out = run(&ws, parent, None, &d3, &mut || {
        Ok(gate_cfg("worker", GATE_WORKFLOW))
    })
    .unwrap();
    assert!(matches!(out, AdvanceOutcome::Terminal), "{out:?}");

    // The gate lifted: the worker's result is now in the parent transcript,
    // its work product transferred, and both inbox messages consumed.
    assert!(parent_wt.join(format!("messages/001-{worker}.md")).exists());
    assert_eq!(
        std::fs::read_to_string(parent_wt.join("out.txt")).unwrap(),
        "the work\n"
    );
    assert!(
        !inbox::inbox_dir(&ws, parent)
            .join(format!("{worker}-001.md"))
            .exists()
    );
    assert!(
        !inbox::inbox_dir(&ws, parent)
            .join(format!("{verifier}-001.md"))
            .exists()
    );
}
