//! Shared fixtures for the runner's two test binaries (ARCH §9.3).
//!
//! A fake [`Agent`] (writing a work-product file when the prompt asks)
//! and a recording fake [`Bundler`] drive every branch of the runner
//! without live model traffic; `setup` and `check` are real shell. They
//! live here rather than in either binary because the split between
//! them is *how many arms an evaluation has*, not what an arm is made
//! of — both sides need the same agent, the same tasks and the same
//! controls.

#![allow(dead_code)]

use agent_eval::agent::{Agent, AgentOutcome, BundleTarget, Bundler, Dispatch};
use agent_eval::experiment::Experiment;
use agent_eval::record::{Controls, TaskRecord};
use agent_eval::runner::{self, EvalConfig};
use agent_eval::suite::Task;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

/// Simulates an agent: writes `out.txt` iff the prompt says "work", and
/// discloses a bundle target iff the prompt says "bundleable". For a
/// "steps" prompt it also fabricates a steps slice in the run home, so
/// the runner's metrics derivation has something to read.
pub struct FakeAgent;
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
pub struct RecordingBundler {
    pub invocations: Mutex<Vec<PathBuf>>,
}
impl Bundler for RecordingBundler {
    fn bundle(&self, _target: &BundleTarget, dest: &Path) -> io::Result<()> {
        self.invocations.lock().unwrap().push(dest.to_path_buf());
        Ok(())
    }
}

#[rustfmt::skip]
pub fn task(id: &str, setup: Option<&str>, prompt: &str, check: &str) -> Task {
    let categories = vec!["early_termination".to_string()];
    let setup = setup.map(str::to_string);
    Task { id: id.to_string(), categories, prompt: prompt.to_string(), setup, check: check.to_string() }
}

#[rustfmt::skip]
pub fn experiment() -> Experiment {
    let workflow = PathBuf::from("/x/workflow.yaml");
    Experiment { name: "baseline".to_string(), workflow }
}

/// One evaluation over one experiment — `evaluate_all`'s general path
/// with a single arm, which is the only entry the runner has since the
/// interleave gave the arms one execution order (bl-b653).
pub fn one_arm(
    tasks: &[Task],
    base: &Path,
    agent: &dyn Agent,
    bundler: Option<&dyn Bundler>,
    cfg: &EvalConfig,
) -> Vec<TaskRecord> {
    let mut records = runner::evaluate_all(
        &[experiment()],
        tasks,
        base,
        agent,
        bundler,
        cfg,
        &controls(),
    )
    .unwrap();
    records.remove(0).tasks
}

pub fn by_id<'a>(records: &'a [TaskRecord], id: &str) -> &'a TaskRecord {
    records.iter().find(|t| t.id == id).unwrap()
}

#[rustfmt::skip]
pub fn controls() -> Controls {
    Controls {
        suite: "tests/suite".to_string(),
        suite_revision: Some("abc123".to_string()),
        fixture_digest: Some("00ff".to_string()),
        driver: "fake-driver".to_string(),
        driver_version: None,
        runs_per_task: 2,
    }
}

#[rustfmt::skip]
pub fn experiment_named(name: &str) -> Experiment {
    Experiment { name: name.to_string(), workflow: PathBuf::from("/x/workflow.yaml") }
}
