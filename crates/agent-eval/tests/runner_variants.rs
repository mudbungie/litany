//! **What an evaluation of several experiments does, and when** (ARCH
//! §9.3; bl-f838's comparison unit, bl-b653's interleave).
//!
//! One invocation is the comparison unit: the arms share the controls
//! and share nothing on disk. Their *execution order* is the subject of
//! the last test here — time is the one control an evaluation cannot
//! declare, so the arms of one cell run adjacent rather than the arms
//! of one variant running to completion.

mod support;

use agent_eval::agent::{Agent, AgentOutcome, Dispatch};
use agent_eval::runner::{self, EvalConfig};
use agent_eval::stats;
use std::io;
use std::sync::Mutex;
use support::{FakeAgent, RecordingBundler, controls, experiment_named, task};

#[test]
fn evaluate_all_runs_every_variant_over_the_same_tasks_in_fresh_dirs() {
    let base = tempfile::tempdir().unwrap();
    // The setup refuses a reused directory — it fails when the seed it
    // writes already exists — so a pass on every (variant, run) proves
    // each got a working directory of its own (the per-variant
    // namespacing under `base/<experiment>`).
    let tasks = vec![task(
        "t",
        Some("test ! -e seed && printf x > seed"),
        "work",
        "test -f out.txt",
    )];
    let cfg = EvalConfig {
        runs: 2,
        bundle_dir: None,
    };
    let experiments = [experiment_named("baseline"), experiment_named("variant")];
    let records = runner::evaluate_all(
        &experiments,
        &tasks,
        base.path(),
        &FakeAgent,
        None,
        &cfg,
        &controls(),
    )
    .unwrap();

    // Order preserved: the first record is the comparison's baseline.
    assert_eq!(records.len(), 2);
    assert_eq!(records[0].provenance.experiment, "baseline");
    assert_eq!(records[1].provenance.experiment, "variant");
    for r in &records {
        // Each variant's provenance carries the one shared controls
        // value; each of its 2 runs passed in a fresh directory.
        assert_eq!(r.provenance.driver, "fake-driver");
        assert_eq!(r.provenance.runs_per_task, 2);
        let m = stats::compute(&r.task_results());
        assert!((m.overall.pass_at_1 - 1.0).abs() < 1e-9);
    }
}

#[test]
fn failing_run_archives_are_namespaced_by_variant() {
    let base = tempfile::tempdir().unwrap();
    let bundle_dir = tempfile::tempdir().unwrap();
    let tasks = vec![task("f", None, "bundleable", "test -f out.txt")];
    let cfg = EvalConfig {
        runs: 1,
        bundle_dir: Some(bundle_dir.path().to_path_buf()),
    };
    let bundler = RecordingBundler::default();
    let experiments = [experiment_named("baseline"), experiment_named("variant")];
    runner::evaluate_all(
        &experiments,
        &tasks,
        base.path(),
        &FakeAgent,
        Some(&bundler),
        &cfg,
        &controls(),
    )
    .unwrap();

    // One failing run per variant, archived under its own name — the
    // two never collide on `f-0`, and each parent directory exists.
    let invocations = bundler.invocations.lock().unwrap();
    assert_eq!(invocations.len(), 2);
    assert_eq!(invocations[0], bundle_dir.path().join("baseline/f-0"));
    assert_eq!(invocations[1], bundle_dir.path().join("variant/f-0"));
    assert!(bundle_dir.path().join("baseline").is_dir());
    assert!(bundle_dir.path().join("variant").is_dir());
}

/// Records the working directory of every dispatch, in order — which
/// carries the variant, the task and the run, so one list is the whole
/// execution order.
#[derive(Default)]
struct OrderingAgent {
    seen: Mutex<Vec<String>>,
}
impl Agent for OrderingAgent {
    fn dispatch(&self, d: &Dispatch) -> io::Result<AgentOutcome> {
        let cell: Vec<String> = d
            .workdir
            .components()
            .rev()
            .skip(1)
            .take(3)
            .map(|c| c.as_os_str().to_string_lossy().into_owned())
            .collect();
        // Reversed back to `<experiment>/<task>/<run>`.
        self.seen
            .lock()
            .unwrap()
            .push(cell.into_iter().rev().collect::<Vec<_>>().join("/"));
        Ok(AgentOutcome { target: None })
    }
}

#[test]
fn the_arms_of_one_cell_run_adjacent_in_time_not_arm_after_arm() {
    // bl-b653: time is the one control an evaluation cannot declare, so
    // the execution order is (task, run) outside and the experiments
    // inside. Every provider change lands between two arms of the same
    // cell rather than between two whole variants.
    let base = tempfile::tempdir().unwrap();
    let tasks = vec![
        task("alpha", None, "work", "true"),
        task("beta", None, "work", "true"),
    ];
    let cfg = EvalConfig {
        runs: 2,
        bundle_dir: None,
    };
    let agent = OrderingAgent::default();
    let experiments = [experiment_named("baseline"), experiment_named("variant")];
    let records = runner::evaluate_all(
        &experiments,
        &tasks,
        base.path(),
        &agent,
        None,
        &cfg,
        &controls(),
    )
    .unwrap();

    assert_eq!(
        *agent.seen.lock().unwrap(),
        [
            "baseline/alpha/0",
            "variant/alpha/0",
            "baseline/alpha/1",
            "variant/alpha/1",
            "baseline/beta/0",
            "variant/beta/0",
            "baseline/beta/1",
            "variant/beta/1",
        ]
    );

    // And the record is what the sequential order wrote: arms in the
    // declared order, each holding both its tasks in suite order, each
    // task holding its runs in run order.
    assert_eq!(records.len(), 2);
    assert_eq!(records[0].provenance.experiment, "baseline");
    assert_eq!(records[1].provenance.experiment, "variant");
    for r in &records {
        let ids: Vec<&str> = r.tasks.iter().map(|t| t.id.as_str()).collect();
        assert_eq!(ids, ["alpha", "beta"]);
        assert!(r.tasks.iter().all(|t| t.runs.len() == 2));
    }
}
