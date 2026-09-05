//! End-to-end coverage for one evaluation arm (ARCH §9.3, bl-36fa):
//! setup pass/fail/absent, check pass/fail, failing-run bundling, and
//! per-run metrics disclosure. The multi-arm half — what an evaluation
//! of several experiments does, and in what order — is
//! `runner_variants.rs`; both drive the fixtures in `support/`.

mod support;

use agent_eval::record;
use agent_eval::runner::EvalConfig;
use agent_eval::stats;
use support::{FakeAgent, RecordingBundler, by_id, one_arm, task};

#[test]
fn evaluate_covers_all_run_shapes() {
    let base = tempfile::tempdir().unwrap();
    let bundle_dir = tempfile::tempdir().unwrap();
    let tasks = vec![
        // setup ok + agent works + check passes.
        task("pass", Some("printf x > seed"), "work", "test -f out.txt"),
        // setup fails -> run counts fail, agent/check never run.
        task("setup-fail", Some("exit 1"), "work", "test -f out.txt"),
        // no setup, check fails, target disclosed -> bundled.
        task("fail-bundle", None, "bundleable", "test -f out.txt"),
        // no setup, check fails, no target -> not bundled.
        task("fail-plain", None, "plain", "test -f out.txt"),
    ];
    let agent = FakeAgent;
    let bundler = RecordingBundler::default();
    let cfg = EvalConfig {
        runs: 5,
        bundle_dir: Some(bundle_dir.path().to_path_buf()),
    };
    let records = one_arm(&tasks, base.path(), &agent, Some(&bundler), &cfg);

    // pass task: 1.0; the other three: 0.0 -> mean-of-means 0.25.
    let m = stats::compute(&record::task_results(&records));
    assert_eq!(m.overall.num_tasks, 4);
    assert_eq!(m.runs_per_task, 5);
    assert!((m.overall.pass_at_1 - 0.25).abs() < 1e-9);

    // A setup-failed run never invoked the driver: no wall, no metrics.
    let sf = &by_id(&records, "setup-fail").runs[0];
    assert!(!sf.pass);
    assert_eq!(sf.wall_ms, 0);
    assert!(sf.metrics.is_none());

    // An undisclosing run has no metrics — missing, not zero.
    assert!(by_id(&records, "pass").runs[0].metrics.is_none());
    // A disclosing run has (empty-tree) metrics: zeros are observed.
    let fb = by_id(&records, "fail-bundle").runs[0].metrics.as_ref();
    assert_eq!(fb.unwrap().attempts, 0);

    // Only the bundleable failing task was archived — once per run.
    let invocations = bundler.invocations.lock().unwrap();
    assert_eq!(invocations.len(), 5);
    assert!(invocations.iter().all(|p| p.starts_with(bundle_dir.path())));
    let first = invocations[0].file_name().unwrap().to_str().unwrap();
    assert!(first.starts_with("fail-bundle-"));
}

#[test]
fn a_disclosing_run_yields_derived_metrics() {
    let base = tempfile::tempdir().unwrap();
    // Discloses a target AND fabricates a steps slice: the runner
    // derives attempts / tool invocations / usage from it.
    let tasks = vec![task("t", None, "bundleable steps work", "test -f out.txt")];
    let cfg = EvalConfig {
        runs: 1,
        bundle_dir: None,
    };
    let records = one_arm(&tasks, base.path(), &FakeAgent, None, &cfg);
    let run = &records[0].runs[0];
    assert!(run.pass);
    let m = run.metrics.as_ref().unwrap();
    assert_eq!(m.attempts, 1);
    assert_eq!(m.tool_invocations, 1);
    assert_eq!(m.input_tokens, Some(7));
    assert_eq!(m.output_tokens, Some(3));
    assert_eq!(m.cache_read_tokens, None);
    assert_eq!(m.cache_write_tokens, None);
    assert_eq!(m.models, vec!["m1".to_string()]);
    // No models.yaml in the run home: providers stay unresolved.
    assert!(m.providers.is_empty());
}

#[test]
fn no_bundling_when_dir_or_bundler_absent() {
    let base = tempfile::tempdir().unwrap();
    // A failing, bundleable task, but neither a bundle dir nor a bundler:
    // the bundle branch is skipped without error.
    let tasks = vec![task("f", None, "bundleable", "test -f out.txt")];
    let cfg = EvalConfig {
        runs: 2,
        bundle_dir: None,
    };
    let records = one_arm(&tasks, base.path(), &FakeAgent, None, &cfg);
    let m = stats::compute(&record::task_results(&records));
    assert_eq!(m.overall.pass_at_1, 0.0);
}
