//! The evaluation orchestrator (ARCH §9.3): experiment × suite × N.
//!
//! For each task, for each of N runs, the runner seeds a fresh isolated
//! workspace (its own `LITANY_HOME` and working directory under `base`),
//! runs the task `setup` (shell), invokes the agent through the [`Agent`]
//! seam, then runs the task `check` (shell) — **exit 0 is the sole pass
//! signal** (§9.1), so success is observable state, never the agent's own
//! claim. Setup, agent, and check share one working directory, as the
//! suite format specifies (`tests/suite/README.md`).
//!
//! Beyond pass/fail, every run records its **outer wall time** — the
//! runner's own measurement around the driver invocation — and, when the
//! driver disclosed a workspace through `LITANY_EVAL_REPORT`, the derived
//! efficiency metrics ([`metrics::collect`], bl-36fa): attempts, tool
//! invocations, and the four canonical usage counters. No disclosure
//! means no metrics (`None`), never zeros.
//!
//! A failing run is optionally archived for triage (§9.2): when a bundle
//! directory is configured and the agent disclosed a [`BundleTarget`],
//! the run's subtree is bundled through the [`Bundler`] seam.
//!
//! Setup, agent invocation, and check are the only impure edges; the
//! aggregation is [`crate::stats::compute`] over
//! [`crate::record::task_results`]. Injecting the agent (and bundler)
//! is what lets the whole path run in tests without live model traffic.

use crate::agent::{Agent, BundleTarget, Bundler, Dispatch};
use crate::experiment::Experiment;
use crate::metrics;
use crate::record::{Controls, Record, RunRecord, TaskRecord};
use crate::suite::Task;
use std::io;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Instant;

/// Knobs for one evaluation.
pub struct EvalConfig {
    /// Runs per task (N ≥ 5 per §9.1).
    pub runs: usize,
    /// When set, failing runs are bundled here for triage (§9.2).
    pub bundle_dir: Option<PathBuf>,
}

/// Run one evaluation per experiment over the same tasks (bl-f838):
/// the comparison unit, experiments × suite × N. The controls are one
/// value for every variant — held equal by construction, not checked
/// after the fact — and each variant's runs are namespaced under
/// `base/<experiment>` so the arms share no run directory. The order
/// given is preserved: the first record is the comparison's baseline.
pub fn evaluate_all(
    experiments: &[Experiment],
    tasks: &[Task],
    base: &Path,
    agent: &dyn Agent,
    bundler: Option<&dyn Bundler>,
    cfg: &EvalConfig,
    controls: &Controls,
) -> io::Result<Vec<Record>> {
    experiments
        .iter()
        .map(|experiment| {
            let tasks = evaluate(
                tasks,
                experiment,
                &base.join(&experiment.name),
                agent,
                bundler,
                cfg,
            )?;
            Ok(Record {
                provenance: controls.provenance(experiment),
                tasks,
            })
        })
        .collect()
}

/// Run the whole evaluation, yielding every task's per-run observations.
pub fn evaluate(
    tasks: &[Task],
    experiment: &Experiment,
    base: &Path,
    agent: &dyn Agent,
    bundler: Option<&dyn Bundler>,
    cfg: &EvalConfig,
) -> io::Result<Vec<TaskRecord>> {
    let mut records = Vec::with_capacity(tasks.len());
    for task in tasks {
        let mut runs = Vec::with_capacity(cfg.runs);
        for run in 0..cfg.runs {
            runs.push(run_once(task, experiment, base, agent, bundler, cfg, run)?);
        }
        records.push(TaskRecord {
            id: task.id.clone(),
            categories: task.categories.clone(),
            runs,
        });
    }
    Ok(records)
}

/// One (task, run): seed, setup, agent (timed), metrics, check.
fn run_once(
    task: &Task,
    experiment: &Experiment,
    base: &Path,
    agent: &dyn Agent,
    bundler: Option<&dyn Bundler>,
    cfg: &EvalConfig,
    run: usize,
) -> io::Result<RunRecord> {
    let dir = base.join(&task.id).join(run.to_string());
    let home = dir.join("home");
    let work = dir.join("work");
    std::fs::create_dir_all(&home)?;
    std::fs::create_dir_all(&work)?;

    // A failed `setup` means the run never got a fair start: count it a
    // fail without invoking the agent or the check. The driver never
    // ran, so there is no wall to measure and nothing disclosed.
    if let Some(setup) = &task.setup
        && !run_shell(setup, &work)?
    {
        return Ok(RunRecord {
            pass: false,
            wall_ms: 0,
            metrics: None,
        });
    }

    let started = Instant::now();
    let outcome = agent.dispatch(&Dispatch {
        prompt: &task.prompt,
        workdir: &work,
        litany_home: &home,
        experiment: &experiment.workflow,
    })?;
    let wall_ms = u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX);
    let run_metrics = outcome
        .target
        .as_ref()
        .map(|t: &BundleTarget| metrics::collect(&t.workspace, &t.agent_id, &home));

    let pass = run_shell(&task.check, &work)?;
    if !pass
        && let (Some(dest_root), Some(b), Some(target)) =
            (&cfg.bundle_dir, bundler, &outcome.target)
    {
        // Namespaced by experiment so the variants of one comparison
        // (bl-f838) never collide on a failing run's archive.
        let dest_dir = dest_root.join(&experiment.name);
        std::fs::create_dir_all(&dest_dir)?;
        let dest = dest_dir.join(format!("{}-{run}", task.id));
        b.bundle(target, &dest)?;
    }
    Ok(RunRecord {
        pass,
        wall_ms,
        metrics: run_metrics,
    })
}

/// Run a shell script in `cwd`; `true` iff it exits 0.
fn run_shell(script: &str, cwd: &Path) -> io::Result<bool> {
    Ok(Command::new("sh")
        .arg("-c")
        .arg(script)
        .current_dir(cwd)
        .status()?
        .success())
}
