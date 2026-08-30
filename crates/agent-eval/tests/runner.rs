//! End-to-end coverage for the orchestrator (ARCH §9.3, bl-36fa).
//!
//! A fake [`Agent`] (writing a work-product file when the prompt asks)
//! and a recording fake [`Bundler`] drive every branch of the runner —
//! setup pass/fail/absent, check pass/fail, failing-run bundling, and
//! per-run metrics disclosure — all without live model traffic. `setup`
//! and `check` are real shell.

use agent_eval::agent::{Agent, AgentOutcome, BundleTarget, Bundler, Dispatch};
use agent_eval::experiment::Experiment;
use agent_eval::record::{self, TaskRecord};
use agent_eval::runner::{self, EvalConfig};
use agent_eval::stats;
use agent_eval::suite::Task;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

/// Simulates an agent: writes `out.txt` iff the prompt says "work", and
/// discloses a bundle target iff the prompt says "bundleable". For a
/// "steps" prompt it also fabricates a steps slice in the run home, so
/// the runner's metrics derivation has something to read.
struct FakeAgent;
impl Agent for FakeAgent {
    fn dispatch(&self, d: &Dispatch) -> io::Result<AgentOutcome> {
        if d.prompt.contains("work") {
            std::fs::write(d.workdir.join("out.txt"), "done")?;
        }
        if d.prompt.contains("steps") {
            let step = d.litany_home.join("steps/fake/001");
            std::fs::create_dir_all(&step)?;
            std::fs::write(
                step.join("response.json"),
                concat!(
                    r#"{"type":"message_start","v":1,"id":null,"model":"m1","role":"assistant"}"#,
                    "\n",
                    r#"{"type":"content_start","index":0,"kind":{"tool_use":{"id":"t","name":"bash"}}}"#,
                    "\n",
                    r#"{"type":"usage","input_tokens":7,"output_tokens":3,"cache_read_tokens":null,"cache_write_tokens":null}"#,
                    "\n",
                    r#"{"type":"end"}"#,
                    "\n"
                ),
            )?;
        }
        let target = d.prompt.contains("bundleable").then(|| BundleTarget {
            workspace: d.litany_home.to_path_buf(),
            agent_id: "fake".to_string(),
        });
        Ok(AgentOutcome { target })
    }
}

/// Records every bundle request.
#[derive(Default)]
struct RecordingBundler {
    invocations: Mutex<Vec<PathBuf>>,
}
impl Bundler for RecordingBundler {
    fn bundle(&self, _target: &BundleTarget, dest: &Path) -> io::Result<()> {
        self.invocations.lock().unwrap().push(dest.to_path_buf());
        Ok(())
    }
}

#[rustfmt::skip]
fn task(id: &str, setup: Option<&str>, prompt: &str, check: &str) -> Task {
    let categories = vec!["early_termination".to_string()];
    let setup = setup.map(str::to_string);
    Task { id: id.to_string(), categories, prompt: prompt.to_string(), setup, check: check.to_string() }
}

#[rustfmt::skip]
fn experiment() -> Experiment {
    let workflow = PathBuf::from("/x/workflow.yaml");
    Experiment { name: "baseline".to_string(), workflow }
}

fn by_id<'a>(records: &'a [TaskRecord], id: &str) -> &'a TaskRecord {
    records.iter().find(|t| t.id == id).unwrap()
}

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
    let records = runner::evaluate(
        &tasks,
        &experiment(),
        base.path(),
        &agent,
        Some(&bundler),
        &cfg,
    )
    .unwrap();

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
    let records =
        runner::evaluate(&tasks, &experiment(), base.path(), &FakeAgent, None, &cfg).unwrap();
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
    let records =
        runner::evaluate(&tasks, &experiment(), base.path(), &FakeAgent, None, &cfg).unwrap();
    let m = stats::compute(&record::task_results(&records));
    assert_eq!(m.overall.pass_at_1, 0.0);
}
