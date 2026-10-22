//! The self-firing seam of the §6 interpreter over a real workspace
//! (`docs/DESIGN_PROCEDURES.md` §2–§3): a hook forks a **procedure
//! child** off the tip it observed, with the generic goal and the
//! inherited dialog; a machinery branch runs no self-firing binding (the
//! bl-08b4 base case, stated of the set); and `user_message` fires where
//! the drain delivered an operator message — under a version-2 config,
//! never under version 1, on both drivers.

use super::advance::{RecLauncher, worker_config};
use super::fixtures::*;
use crate::config::version::Version;
use crate::config::{Event, Workflow};
use crate::prompt::dispatch::advance::{AdvanceOutcome, run};
use crate::prompt::resolve::WorkerConfig;
use crate::prompt::workflow_actions::run_step_hook;
use crate::prompt::{Deps, Error, SystemClock, inbox};
use crate::template::{GitRunner, RealGit};
use crate::workspace::agent_name::mint::test_rng;
use crate::workspace::{agent_worktree, fixture};
use std::path::Path;
use std::sync::atomic::AtomicBool;

mod user_message;

const PARENT: &str = "20260101-a1";

fn workflow(yaml: &str) -> Workflow {
    Workflow::parse(yaml, Path::new("workflow.yaml")).unwrap()
}

/// The shared components a real-workspace hop borrows (the
/// [`super::verifier_gate`] shape), varying only the adapter per test.
struct Parts {
    adapter: StubAdapter,
    sleeper: StubSleeper,
    git: RealGit,
    clock: SystemClock,
    id: FixedIdGen,
    tools: StubToolExecutor,
    stop: AtomicBool,
    launcher: RecLauncher,
    cfg_root: tempfile::TempDir,
}

impl Parts {
    fn new(adapter: StubAdapter) -> Self {
        Self {
            adapter,
            sleeper: StubSleeper::default(),
            git: RealGit::new(),
            clock: SystemClock,
            id: FixedIdGen,
            tools: StubToolExecutor::ok(),
            stop: AtomicBool::new(false),
            launcher: RecLauncher::default(),
            cfg_root: tempfile::TempDir::new().unwrap(),
        }
    }
    fn deps(&self) -> Deps<'_> {
        Deps {
            adapter: &self.adapter,
            sleeper: &self.sleeper,
            git: &self.git,
            clock: &self.clock,
            id_gen: &self.id,
            tool_executor: &self.tools,
            config_root: self.cfg_root.path(),
            data_root: self.cfg_root.path(),
            adapter_target: None,
            stop: &self.stop,
            launcher: &self.launcher,
            rng: test_rng(),
        }
    }
}

/// A root with one delivered operator entry committed — the dialog a
/// procedure child inherits.
fn root_with_dialog(ws: &Path) -> std::path::PathBuf {
    let wt = fixture::spawn_root(ws, PARENT);
    let git = RealGit::new();
    std::fs::create_dir_all(wt.join("messages")).unwrap();
    std::fs::write(wt.join("messages/001-user.md"), "name this\n").unwrap();
    git.run(&wt, &["add", "-A"]).unwrap();
    git.run(&wt, &["commit", "-m", "dialog"]).unwrap();
    wt
}

/// Whether the §6 `notify_ui` mark stands for `agent` in `wt`'s repo.
fn notified(wt: &Path, agent: &str) -> bool {
    RealGit::new()
        .run(
            wt,
            &[
                "show-ref",
                "--verify",
                "--quiet",
                &format!("refs/litany/notify/{agent}"),
            ],
        )
        .is_ok()
}

#[test]
fn a_hook_forks_a_procedure_child_off_the_tip_with_the_generic_goal_and_the_dialog() {
    // §2: `pre_step: [dispatch(titler)]` forks a titler off the tip the
    // hook observed, instructed by the generic goal (its soul says the
    // rest), and — being machinery — keeping the dialog it is to read.
    let (_h, ws) = fixture::workspace();
    const HOOK: &str = "events:\n  pre_step:\n    - dispatch(titler)\n";
    fixture::amend_config(
        &ws,
        &[
            ("souls/titler.md", "Name the conversation."),
            ("workflow.yaml", HOOK),
        ],
    );
    let wt = root_with_dialog(&ws);
    let parts = Parts::new(unreachable_adapter());
    let wf = workflow(HOOK);
    run_step_hook(
        &ws,
        PARENT,
        &wt,
        &wf,
        Version(2),
        "worker",
        Event::PreStep,
        &parts.deps(),
    )
    .unwrap();

    let launched = parts.launcher.invocations.borrow().clone();
    assert_eq!(launched.len(), 1, "one fork: {launched:?}");
    let child_wt = agent_worktree(&ws, &launched[0]);
    let goal = std::fs::read_to_string(child_wt.join("goal.md")).unwrap();
    assert!(
        goal.starts_with(&format!("You are the titler for branch `{PARENT}`.")),
        "{goal}"
    );
    assert_eq!(
        std::fs::read_to_string(child_wt.join("soul.md")).unwrap(),
        "Name the conversation."
    );
    assert!(
        child_wt.join("messages/001-user.md").exists(),
        "a procedure child keeps the inherited dialog (ARCH §2.2)"
    );
    // Off the tip: the child's dispatch commit is parented on the
    // parent's HEAD.
    let git = RealGit::new();
    let tip = git.run_capture(&wt, &["rev-parse", "HEAD"]).unwrap();
    let parent_of_child = git
        .run_capture(
            &wt,
            &[
                "rev-parse",
                &format!("{}~1", crate::workspace::agent_ref(&launched[0])),
            ],
        )
        .unwrap();
    assert_eq!(parent_of_child, tip);
}

#[test]
fn a_machinery_branch_runs_no_self_firing_binding() {
    // THE PIN, of the set (bl-08b4): the titler inherits this workflow,
    // and its own `pre_step` must not fork a titler. Nothing is launched
    // and nothing is marked — not even the ref mark bound beside it.
    let (_h, ws) = fixture::workspace();
    let wt = root_with_dialog(&ws);
    let parts = Parts::new(unreachable_adapter());
    let wf = workflow("events:\n  pre_step:\n    - dispatch(titler)\n    - notify_ui\n");
    run_step_hook(
        &ws,
        PARENT,
        &wt,
        &wf,
        Version(2),
        "titler",
        Event::PreStep,
        &parts.deps(),
    )
    .unwrap();
    assert!(parts.launcher.invocations.borrow().is_empty());
    assert!(!notified(&wt, PARENT));
}

#[test]
fn a_ref_mark_in_a_hook_still_marks_and_a_landing_is_declined() {
    let (_h, ws) = fixture::workspace();
    let wt = root_with_dialog(&ws);
    let parts = Parts::new(unreachable_adapter());
    let wf = workflow("events:\n  post_step:\n    - notify_ui\n");
    run_step_hook(
        &ws,
        PARENT,
        &wt,
        &wf,
        Version(2),
        "worker",
        Event::PostStep,
        &parts.deps(),
    )
    .unwrap();
    assert!(notified(&wt, PARENT));

    // A landing is minted from a child's return; a hook has none.
    let wf = workflow("events:\n  post_step:\n    - land_compaction\n");
    let err = run_step_hook(
        &ws,
        PARENT,
        &wt,
        &wf,
        Version(2),
        "worker",
        Event::PostStep,
        &parts.deps(),
    )
    .unwrap_err();
    assert!(
        matches!(err, Error::ActionUnsupported { ref event, .. } if event == "post_step"),
        "{err:?}"
    );
}
