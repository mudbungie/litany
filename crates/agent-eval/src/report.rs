//! Rendering one evaluation record as a text report (ARCH §9.1, §9.3).
//!
//! The report leads with pass@1 (the optimization target) and its 95%
//! Wilson interval, then pass@5, then the per-category breakdown — the
//! §9.1 quality metrics, unchanged — followed by the efficiency
//! aggregates (outer wall time, attempts, tool invocations, the four
//! canonical usage counters; bl-36fa) and the reproducibility inputs.
//! The v0.9 baseline criterion — ~40% ± 5% pass@1 on the suite — is
//! printed as a reference line; whether a given run meets it is read
//! off the interval, not asserted here (this crate never runs a live
//! model). An unreported value renders as `—`, never as 0.

use crate::compare;
use crate::metrics::Efficiency;
use crate::record::Record;
use crate::stats::{self, Summary};

/// Render an evaluation's whole product (bl-f838): one record is the
/// single-experiment report below; several are the side-by-side —
/// the first record is the baseline, and each later one renders as a
/// baseline → candidate [`compare`] block over the same suite.
pub fn render_all(records: &[Record]) -> String {
    match records {
        [one] => render(one),
        [baseline, candidates @ ..] => candidates
            .iter()
            .map(|candidate| compare::render(baseline, candidate))
            .collect::<Vec<String>>()
            .join("\n"),
        [] => String::new(),
    }
}

/// Render the full report for one evaluation record.
pub fn render(record: &Record) -> String {
    let m = stats::compute(&record.task_results());
    let mut out = String::new();
    out.push_str(&format!(
        "experiment: {}\ntasks: {}   runs/task: {}\n\n",
        record.provenance.experiment, m.overall.num_tasks, m.runs_per_task
    ));
    out.push_str(&format!(
        "pass@1 (reliability): {}\n",
        pass1_line(&m.overall)
    ));
    out.push_str(&format!(
        "pass@5 (capability):  {:.1}% of tasks\n\n",
        m.overall.pass_at_5 * 100.0
    ));
    out.push_str("per category (§9.1):\n");
    for cat in &m.categories {
        out.push_str(&format!(
            "  {:<20} n={:<3} pass@1 {}  pass@5 {:.1}%\n",
            cat.tag,
            cat.summary.num_tasks,
            pass1_line(&cat.summary),
            cat.summary.pass_at_5 * 100.0
        ));
    }
    out.push_str(&efficiency_block(record));
    out.push_str(&reproducibility_block(record));
    out.push_str(
        "\nbaseline criterion (§9.1, v0.9): pass@1 ~40% ± 5% (Wilson CI) on the full suite\n",
    );
    out
}

/// `NN.N% [lo, hi]` (percentages), the shared pass@1 rendering.
pub fn pass1_line(s: &Summary) -> String {
    format!(
        "{:.1}% [{:.1}%, {:.1}%]",
        s.pass_at_1 * 100.0,
        s.pass_at_1_ci.lo * 100.0,
        s.pass_at_1_ci.hi * 100.0
    )
}

/// The efficiency section: outer wall over every run; attempts, tool
/// invocations, and usage only over runs whose driver disclosed a
/// workspace (`—` when none did — missing, not zero).
fn efficiency_block(record: &Record) -> String {
    let e = Efficiency::over(record.tasks.iter().flat_map(|t| &t.runs));
    format!(
        "\nefficiency (bl-36fa; — = unreported):\n  \
         runs with workspace metrics: {}/{}\n  \
         outer wall/run: {:.1}s\n  \
         attempts/run: {}   tool invocations/run: {}\n  \
         usage totals: input {}  output {}  cache_read {}  cache_write {}\n",
        e.disclosed,
        e.runs,
        e.wall_mean_s(),
        opt_mean(e.attempts_mean()),
        opt_mean(e.tools_mean()),
        opt_count(e.input_tokens),
        opt_count(e.output_tokens),
        opt_count(e.cache_read_tokens),
        opt_count(e.cache_write_tokens),
    )
}

/// The reproducibility section (shared with the comparison report).
pub fn reproducibility_block(record: &Record) -> String {
    let p = &record.provenance;
    let models = record.observed_models();
    let providers = record.observed_providers();
    format!(
        "\nreproducibility:\n  \
         suite: {} @ {}\n  \
         starting fixture: {}\n  \
         experiment: {} ({})\n  \
         driver: {} ({})\n  \
         models: {}   providers: {}\n  \
         runs/task: {}\n",
        p.suite,
        p.suite_revision.as_deref().unwrap_or("revision unknown"),
        match &p.fixture_digest {
            Some(d) => format!("sha256:{d}"),
            None => "unknown".to_string(),
        },
        p.experiment,
        p.workflow,
        p.driver,
        p.driver_version.as_deref().unwrap_or("version unreported"),
        joined(&models),
        joined(&providers),
        p.runs_per_task,
    )
}

/// A mean over disclosed runs, or `—` when nothing was disclosed.
pub fn opt_mean(v: Option<f64>) -> String {
    match v {
        Some(v) => format!("{v:.1}"),
        None => "—".to_string(),
    }
}

/// A usage-counter total, or `—` when it was never reported.
pub fn opt_count(v: Option<u64>) -> String {
    match v {
        Some(v) => v.to_string(),
        None => "—".to_string(),
    }
}

/// A comma-joined observation set, or `unreported` when empty.
fn joined(items: &[String]) -> String {
    if items.is_empty() {
        "unreported".to_string()
    } else {
        items.join(", ")
    }
}
