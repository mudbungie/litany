//! The `agent-eval` binary (ARCH §9.3, v0.10; bl-36fa).
//!
//! `agent-eval run --config <experiment> --suite <suite> --runs N
//! --agent <driver>` executes the experiment
//! (`experiments/<config>/workflow.yaml`) against the suite (a task
//! directory) N times per task and prints quality (pass@1/pass@5, §9.1),
//! efficiency (outer wall, attempts, tool invocations, usage), and the
//! reproducibility inputs. `--record <path>` additionally saves the
//! machine-readable evaluation record.
//!
//! `--config` is repeatable (bl-f838): several run the same suite under
//! each named experiment in one invocation — the first is the baseline
//! — and print the baseline → candidate comparison per later variant.
//! The controls (suite, fixtures, driver, N) are given once, so the
//! variants differ in exactly the workflow; `--record <path>` then
//! names a directory receiving one `<experiment>.json` per variant.
//!
//! `agent-eval compare <baseline.json> <candidate.json>` renders the
//! baseline → candidate deltas from two saved records — no run happens;
//! comparison never invokes a driver or a model.
//!
//! `--agent` names the external harness-driver the runner invokes per run
//! (the injectable agent seam, §9.3). It is **required with no default**:
//! which driver runs the agent under test is an experiment-defining
//! input, so it is named at every invocation. The shipped driver is
//! `litany-eval-agent` (`crates/litany-eval-agent`); any program
//! honouring the contract in the repo README ("Run the suite") works.
//! Clap rejects a missing `--agent` up front rather than letting the
//! runner die on a failed spawn per task.
//!
//! `--bundle-dir` archives failing runs for triage via `litany bundle`
//! (§9.2). This file is thin wiring over the library (`lib.rs`); all
//! logic and its coverage live there.

use agent_eval::agent::{CommandAgent, CommandBundler};
use agent_eval::record::{self, Controls, Record};
use agent_eval::runner::{self, EvalConfig};
use agent_eval::{compare, experiment, report, repro, suite};
use clap::{Parser, Subcommand};
use std::path::PathBuf;
use std::process::ExitCode;

#[derive(Parser)]
#[command(
    name = "agent-eval",
    about = "litany evaluation runner (ARCH §9.3)",
    version
)]
struct Cli {
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Run experiment × suite × N and report quality + efficiency.
    Run(RunArgs),
    /// Compare two saved records: baseline → candidate deltas.
    Compare {
        /// The baseline evaluation record (`run --record`).
        baseline: PathBuf,
        /// The candidate evaluation record.
        candidate: PathBuf,
    },
}

#[derive(Parser)]
struct RunArgs {
    /// Experiment name: `experiments/<config>/workflow.yaml` (§9.3).
    /// Repeatable (bl-f838): several run the same suite under each —
    /// the first is the baseline — and report the comparisons.
    #[arg(long, required = true)]
    config: Vec<String>,
    /// Path to the task-suite directory (e.g. `tests/suite`, §9.1).
    #[arg(long)]
    suite: PathBuf,
    /// Runs per task (N ≥ 5 per §9.1).
    #[arg(long)]
    runs: usize,
    /// Directory holding the experiments (default `experiments`).
    #[arg(long, default_value = "experiments")]
    experiments_dir: PathBuf,
    /// External harness-driver invoked per run (the agent seam, §9.3).
    /// Required, no default — the driver is an experiment-defining
    /// input. The shipped one is `litany-eval-agent`; the contract any
    /// driver must honour is in the repo README, "Run the suite".
    #[arg(long)]
    agent: String,
    /// Archive failing runs here for triage via `litany bundle` (§9.2).
    #[arg(long)]
    bundle_dir: Option<PathBuf>,
    /// The `litany` binary used to bundle failing runs (§9.2).
    #[arg(long, default_value = "litany")]
    litany: String,
    /// Save the machine-readable evaluation record here (bl-36fa) — the
    /// input `agent-eval compare` consumes. With one `--config`, a
    /// file; with several, a directory receiving one
    /// `<experiment>.json` per variant (bl-f838).
    #[arg(long)]
    record: Option<PathBuf>,
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    let result = match cli.cmd {
        Cmd::Run(args) => run(args),
        Cmd::Compare {
            baseline,
            candidate,
        } => run_compare(&baseline, &candidate),
    };
    match result {
        Ok(text) => {
            print!("{text}");
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("agent-eval: {e}");
            ExitCode::FAILURE
        }
    }
}

fn run(cli: RunArgs) -> Result<String, Box<dyn std::error::Error>> {
    let experiments = experiment::resolve_all(&cli.config, &cli.experiments_dir)?;
    let tasks = suite::load(&cli.suite)?;
    let base = tempfile::tempdir()?;

    let agent = CommandAgent::new(&cli.agent);
    let bundler = CommandBundler::new(&cli.litany);
    let cfg = EvalConfig {
        runs: cli.runs,
        bundle_dir: cli.bundle_dir,
    };
    // The controls are probed exactly once per invocation — every
    // variant of the comparison shares them by construction (bl-f838).
    let controls = Controls {
        suite: cli.suite.display().to_string(),
        suite_revision: repro::suite_revision(&cli.suite),
        fixture_digest: repro::fixture_digest(&cli.suite),
        driver: cli.agent.clone(),
        driver_version: repro::driver_version(&cli.agent),
        runs_per_task: cli.runs,
    };
    let records = runner::evaluate_all(
        &experiments,
        &tasks,
        base.path(),
        &agent,
        Some(&bundler),
        &cfg,
        &controls,
    )?;
    if let Some(path) = &cli.record {
        record::save_all(&records, path)?;
    }
    Ok(report::render_all(&records))
}

fn run_compare(
    baseline: &std::path::Path,
    candidate: &std::path::Path,
) -> Result<String, Box<dyn std::error::Error>> {
    let baseline = Record::load(baseline)?;
    let candidate = Record::load(candidate)?;
    Ok(compare::render(&baseline, &candidate))
}
