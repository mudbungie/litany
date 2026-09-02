//! The §6 budget gate at the one dispatch fork ([`super::super::run`]):
//! `max_depth` refuses the child that would breach it *before* the branch
//! exists, and a harness-initiated dispatch ([`super::super::run_procedure`])
//! reports the refusal instead of failing the dispatching branch.

use super::*;

/// A `workflow.yaml` whose only declared budget is `max_depth: <n>`.
fn workflow_with_max_depth(n: u32) -> String {
    format!("events: {{}}\nbudgets:\n  max_depth: {n}\n")
}

#[test]
fn max_depth_refuses_the_dispatch_that_would_breach_it() {
    // THE PIN (§6): the ceiling is enforced at the fork, not at the
    // child's first model call — so the branch that would breach it is
    // never created. `max_depth: 0` makes the root's own first dispatch
    // (depth 1) the breach.
    let (_h, ws) = fixture::workspace();
    fixture::amend_config(&ws, &[("workflow.yaml", &workflow_with_max_depth(0))]);
    let parent_wt = fixture::spawn_root(&ws, "20260101-p1");
    let g = crate::template::RealGit::new();
    let launcher = RecordingLauncher::ok();
    let err = run(
        &req(&ws, "20260101-p1", &parent_wt, "g"),
        &g,
        &crate::prompt::SystemClock,
        &crate::prompt::NanoIdGen,
        &launcher,
        &test_rng(),
    )
    .unwrap_err();
    let Error::DispatchRefused {
        child,
        parent,
        exhausted,
    } = &err
    else {
        panic!("{err:?}");
    };
    assert!(child.starts_with("20260101-p1-"), "{child}");
    assert_eq!(parent, "20260101-p1");
    assert_eq!(exhausted.axis, crate::prompt::budget::Axis::Depth);
    assert_eq!((exhausted.limit, exhausted.actual), (0, 1));
    // The message names the axis and the config that declared it.
    let rendered = err.to_string();
    assert!(rendered.contains("max_depth"), "{rendered}");
    assert!(rendered.contains("workflow.yaml"), "{rendered}");

    // Nothing was created: no ref, no worktree, no inbox, no launch.
    assert_eq!(workspace::agent_ids(&ws, &g).unwrap(), vec!["20260101-p1"]);
    assert!(!ws.join("agents").join(child).exists());
    assert!(!inbox::inbox_dir(&ws, child).exists());
    assert!(launcher.invocations.borrow().is_empty());
}

#[test]
fn a_dispatch_within_the_depth_ceiling_forks_normally() {
    // The boundary: `max_depth` is the deepest *allowed* depth, so a
    // child landing exactly on it is dispatched (§6).
    let (_h, ws) = fixture::workspace();
    fixture::amend_config(&ws, &[("workflow.yaml", &workflow_with_max_depth(1))]);
    let parent_wt = fixture::spawn_root(&ws, "20260101-p1");
    run(
        &req(&ws, "20260101-p1", &parent_wt, "g"),
        &crate::template::RealGit::new(),
        &crate::prompt::SystemClock,
        &crate::prompt::NanoIdGen,
        &RecordingLauncher::ok(),
        &test_rng(),
    )
    .unwrap();
}

#[test]
fn a_workflow_triggered_dispatch_reports_a_refusal_and_does_not_fail_the_branch() {
    // The harness's own procedure dispatches (§6 `worker_flush`, the
    // verifier gate) share the gate but not its verdict: a refusal is the
    // operator's declared ceiling, not the dispatching branch's failure.
    let (_h, ws) = fixture::workspace();
    fixture::amend_config(&ws, &[("workflow.yaml", &workflow_with_max_depth(0))]);
    let parent_wt = fixture::spawn_root(&ws, "20260101-p1");
    let g = crate::template::RealGit::new();
    let launcher = RecordingLauncher::ok();
    run_procedure(
        &req(&ws, "20260101-p1", &parent_wt, "g"),
        &g,
        &crate::prompt::SystemClock,
        &crate::prompt::NanoIdGen,
        &launcher,
        &test_rng(),
    )
    .unwrap();
    assert_eq!(workspace::agent_ids(&ws, &g).unwrap(), vec!["20260101-p1"]);
    assert!(launcher.invocations.borrow().is_empty());
}

#[test]
fn a_workflow_triggered_dispatch_forks_and_still_propagates_other_errors() {
    let (_h, ws) = fixture::workspace();
    let parent_wt = fixture::spawn_root(&ws, "20260101-p1");
    let g = crate::template::RealGit::new();
    // Within budget: the procedure dispatch forks like any other.
    run_procedure(
        &req(&ws, "20260101-p1", &parent_wt, "g"),
        &g,
        &crate::prompt::SystemClock,
        &crate::prompt::NanoIdGen,
        &RecordingLauncher::ok(),
        &test_rng(),
    )
    .unwrap();
    // A non-budget failure is still the branch's to handle.
    let err = run_procedure(
        &req(&ws, "20260101-p1", &parent_wt, "g"),
        &g,
        &crate::prompt::SystemClock,
        &crate::prompt::NanoIdGen,
        &RecordingLauncher::failing(),
        &test_rng(),
    )
    .unwrap_err();
    assert!(matches!(err, Error::ExecutorLock { .. }), "{err:?}");
}

#[test]
fn an_unreadable_workflow_declines_the_dispatch_in_the_control_read_voice() {
    // The budget gate reads `workflow.yaml` from the frozen config commit
    // like every other control read (§2.2); a read that fails is named
    // by its `<commit>:<path>` address, not swallowed into an unbudgeted
    // dispatch.
    let (_h, ws) = fixture::workspace();
    let parent_wt = fixture::spawn_root(&ws, "20260101-p1");
    let err = run(
        &req(&ws, "20260101-p1", &parent_wt, "g"),
        &FailOnWorkflowShow(crate::template::RealGit::new()),
        &crate::prompt::SystemClock,
        &crate::prompt::NanoIdGen,
        &RecordingLauncher::ok(),
        &test_rng(),
    )
    .unwrap_err();
    let Error::ControlRead { path, .. } = &err else {
        panic!("{err:?}");
    };
    assert!(
        path.to_string_lossy().ends_with(":workflow.yaml"),
        "{path:?}"
    );
}

#[test]
fn a_malformed_workflow_declines_the_dispatch() {
    let (_h, ws) = fixture::workspace();
    fixture::amend_config(&ws, &[("workflow.yaml", "budgets: [not, a, map]\n")]);
    let parent_wt = fixture::spawn_root(&ws, "20260101-p1");
    let err = run(
        &req(&ws, "20260101-p1", &parent_wt, "g"),
        &crate::template::RealGit::new(),
        &crate::prompt::SystemClock,
        &crate::prompt::NanoIdGen,
        &RecordingLauncher::ok(),
        &test_rng(),
    )
    .unwrap_err();
    assert!(matches!(err, Error::Config(_)), "{err:?}");
}

/// Real git for everything except `git show <commit>:workflow.yaml`,
/// which fails — so the budget gate's own control read is the only thing
/// that breaks.
struct FailOnWorkflowShow(crate::template::RealGit);

impl crate::template::GitRunner for FailOnWorkflowShow {
    fn run(&self, dest: &Path, args: &[&str]) -> io::Result<()> {
        self.0.run(dest, args)
    }
    fn run_capture(&self, dest: &Path, args: &[&str]) -> io::Result<String> {
        if args.iter().any(|a| a.ends_with(":workflow.yaml")) {
            return Err(io::Error::other("stub: workflow.yaml unreadable"));
        }
        self.0.run_capture(dest, args)
    }
}

#[test]
fn the_gate_reads_the_dispatching_branchs_workflow_mark_over_the_followed_tip() {
    // §6 *The workflow mark* × §2.2 follow-the-tip (bl-403b): the gate
    // and the child's own step checks must be one answer. The lineage's
    // current tip declares `max_depth: 0`; the parent's standing mark
    // pins the fork commit's unbounded workflow — so the fork is
    // allowed under the mark, and refused under the tip once the mark
    // is cleared.
    let (_h, ws) = fixture::workspace();
    let parent_wt = fixture::spawn_root(&ws, "20260101-p1");
    let g = crate::template::RealGit::new();
    let fork = g
        .run_capture(
            &workspace::repo_git(&ws),
            &["rev-parse", &workspace::config_ref("default")],
        )
        .unwrap()
        .trim()
        .to_string();
    fixture::amend_config(&ws, &[("workflow.yaml", &workflow_with_max_depth(0))]);
    workspace::workflow_mark::write(&ws, "20260101-p1", &fork, &g).unwrap();
    run(
        &req(&ws, "20260101-p1", &parent_wt, "g"),
        &g,
        &crate::prompt::SystemClock,
        &crate::prompt::NanoIdGen,
        &RecordingLauncher::ok(),
        &test_rng(),
    )
    .unwrap();
    // Cleared, the followed tip's ceiling answers again.
    workspace::workflow_mark::clear(&ws, "20260101-p1", &g).unwrap();
    let err = run(
        &req(&ws, "20260101-p1", &parent_wt, "g"),
        &g,
        &crate::prompt::SystemClock,
        &crate::prompt::NanoIdGen,
        &RecordingLauncher::ok(),
        &test_rng(),
    )
    .unwrap_err();
    assert!(matches!(err, Error::DispatchRefused { .. }), "{err:?}");
}
