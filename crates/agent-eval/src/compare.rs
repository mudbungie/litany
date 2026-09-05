//! Baseline → candidate comparison (bl-36fa, ARCH §9.3): the neutral
//! measurement contract behind cross-harness claims. Two evaluation
//! records — same suite, same fixtures, possibly different drivers —
//! are compared task-by-task; the report gives per-task, per-category,
//! and total deltas over quality (pass@1/pass@5, Wilson intervals
//! preserved) and efficiency (outer wall, attempts, tool invocations,
//! four usage counters). Deltas are computed only over the shared task
//! set; tasks present on one side alone are named, never silently
//! dropped. A metric missing on either side yields `Δ —`, never a
//! fabricated zero, and nothing here infers price. A `controls:` line
//! (bl-f838) derives whether the two records held everything but the
//! experiment equal — one invocation's variants always did, records
//! from separate invocations get told what differed.

use crate::metrics::Efficiency;
use crate::paired;
use crate::record::{self, Record, TaskRecord};
use crate::report;
use crate::stats::{self, Metrics};
use std::collections::BTreeMap;

/// Render the whole comparison report.
pub fn render(baseline: &Record, candidate: &Record) -> String {
    let b_by_id: BTreeMap<&str, &TaskRecord> =
        baseline.tasks.iter().map(|t| (t.id.as_str(), t)).collect();
    let c_by_id: BTreeMap<&str, &TaskRecord> =
        candidate.tasks.iter().map(|t| (t.id.as_str(), t)).collect();
    let shared_b: Vec<TaskRecord> = baseline
        .tasks
        .iter()
        .filter(|t| c_by_id.contains_key(t.id.as_str()))
        .cloned()
        .collect();
    let shared_c: Vec<TaskRecord> = candidate
        .tasks
        .iter()
        .filter(|t| b_by_id.contains_key(t.id.as_str()))
        .cloned()
        .collect();
    let mb = stats::compute(&record::task_results(&shared_b));
    let mc = stats::compute(&record::task_results(&shared_c));

    let mut out = format!(
        "baseline {} → candidate {}: comparison over {} shared task(s)\n",
        baseline.provenance.experiment,
        candidate.provenance.experiment,
        shared_b.len()
    );
    out.push_str("\nbaseline:");
    out.push_str(&indent(&report::reproducibility_block(baseline)));
    out.push_str("\ncandidate:");
    out.push_str(&indent(&report::reproducibility_block(candidate)));
    out.push_str(&controls_line(baseline, candidate));
    out.push_str(&total_block(&mb, &mc, &shared_b, &shared_c));
    out.push_str(&significance_block(&shared_b, &shared_c));
    out.push_str(&category_block(&mb, &mc));
    out.push_str(&task_block(&shared_b, &c_by_id));
    out.push_str(&unmatched_block("baseline", &baseline.tasks, &c_by_id));
    out.push_str(&unmatched_block("candidate", &candidate.tasks, &b_by_id));
    out
}

/// Whether the two records' controls — every reproducibility input
/// other than the experiment (`record::controls_diff`, bl-f838) — were
/// held equal, plus the observed model sets when both sides reported
/// one. A difference is a fact to see, never a refusal: cross-time and
/// cross-harness comparison is this report's charter, and the line is
/// what says the deltas below compare more than the experiment.
fn controls_line(baseline: &Record, candidate: &Record) -> String {
    let mut diffs = record::controls_diff(&baseline.provenance, &candidate.provenance);
    let (mb, mc) = (baseline.observed_models(), candidate.observed_models());
    if !mb.is_empty() && !mc.is_empty() && mb != mc {
        diffs.push("observed models");
    }
    if diffs.is_empty() {
        "\ncontrols: held — the experiment is the only declared difference\n".to_string()
    } else {
        format!(
            "\ncontrols differ: {} — the deltas below compare more than the experiment\n",
            diffs.join(", ")
        )
    }
}

/// Total quality and efficiency deltas over the shared tasks.
fn total_block(mb: &Metrics, mc: &Metrics, b: &[TaskRecord], c: &[TaskRecord]) -> String {
    let eb = Efficiency::over(b.iter().flat_map(|t| &t.runs));
    let ec = Efficiency::over(c.iter().flat_map(|t| &t.runs));
    format!(
        "\ntotal:\n  \
         pass@1: {} → {}   Δ {:+.1}\n  \
         pass@5: {:.1}% → {:.1}%   Δ {:+.1}\n  \
         outer wall/run: {:.1}s → {:.1}s   Δ {:+.1}s\n  \
         attempts/run: {} → {}   Δ {}\n  \
         tool invocations/run: {} → {}   Δ {}\n  \
         usage totals: input {}  output {}  cache_read {}  cache_write {}\n",
        report::pass1_line(&mb.overall),
        report::pass1_line(&mc.overall),
        (mc.overall.pass_at_1 - mb.overall.pass_at_1) * 100.0,
        mb.overall.pass_at_5 * 100.0,
        mc.overall.pass_at_5 * 100.0,
        (mc.overall.pass_at_5 - mb.overall.pass_at_5) * 100.0,
        eb.wall_mean_s(),
        ec.wall_mean_s(),
        ec.wall_mean_s() - eb.wall_mean_s(),
        report::opt_mean(eb.attempts_mean()),
        report::opt_mean(ec.attempts_mean()),
        opt_delta(eb.attempts_mean(), ec.attempts_mean()),
        report::opt_mean(eb.tools_mean()),
        report::opt_mean(ec.tools_mean()),
        opt_delta(eb.tools_mean(), ec.tools_mean()),
        counter_pair(eb.input_tokens, ec.input_tokens),
        counter_pair(eb.output_tokens, ec.output_tokens),
        counter_pair(eb.cache_read_tokens, ec.cache_read_tokens),
        counter_pair(eb.cache_write_tokens, ec.cache_write_tokens),
    )
}

/// The paired verdict on the pass@1 delta (bl-a35e,
/// [`crate::paired`]): one exact two-sided sign test over the shared
/// tasks' pass rates, its counts, and the method named beside its
/// assumptions — a delta with no answer about whether it is real is
/// what the block above gives on its own, and the §12 criterion asks
/// for the answer.
///
/// The pairs come from the two shared-task lists, which `render` built
/// in the *same* order from the baseline's own ordering, so index `i`
/// is one task on both sides.
fn significance_block(b: &[TaskRecord], c: &[TaskRecord]) -> String {
    let pairs: Vec<(f64, f64)> = b
        .iter()
        .zip(c)
        .map(|(tb, tc)| (pass_rate(tb), pass_rate(tc)))
        .collect();
    let t = paired::sign_test(&pairs);
    let verdict = match t.p_value {
        None => "no task moved — no verdict".to_string(),
        Some(p) => format!(
            "p = {p:.4} ({}significant at alpha = {})",
            if t.significant() { "" } else { "not " },
            paired::ALPHA,
        ),
    };
    format!(
        "\npass@1 significance: {verdict}\n  \
         {} better, {} worse, {} tied of {} shared task(s)\n  \
         exact two-sided sign test, paired per task over per-task pass rates; ties \
         discarded. Paired because runs cluster within a task, so pooling every run \
         would treat them as independent trials; directional, so it reads which way \
         each task moved and never how far\n",
        t.better,
        t.worse,
        t.tied,
        pairs.len(),
    )
}

/// Per-category pass@1/pass@5 deltas (categories re-derived per side
/// over the shared tasks; a tag absent from one side prints `—`).
fn category_block(mb: &Metrics, mc: &Metrics) -> String {
    let b: BTreeMap<&str, _> = mb.categories.iter().map(|c| (c.tag.as_str(), c)).collect();
    let c: BTreeMap<&str, _> = mc.categories.iter().map(|c| (c.tag.as_str(), c)).collect();
    let mut out = String::from("\nper category:\n");
    let mut tags: Vec<&str> = b.keys().chain(c.keys()).copied().collect();
    tags.sort_unstable();
    tags.dedup();
    for tag in tags {
        let line = match (b.get(tag), c.get(tag)) {
            (Some(cb), Some(cc)) => format!(
                "pass@1 {:.1}% → {:.1}% (Δ {:+.1})  pass@5 {:.1}% → {:.1}% (Δ {:+.1})",
                cb.summary.pass_at_1 * 100.0,
                cc.summary.pass_at_1 * 100.0,
                (cc.summary.pass_at_1 - cb.summary.pass_at_1) * 100.0,
                cb.summary.pass_at_5 * 100.0,
                cc.summary.pass_at_5 * 100.0,
                (cc.summary.pass_at_5 - cb.summary.pass_at_5) * 100.0,
            ),
            _ => "present on one side only — Δ —".to_string(),
        };
        out.push_str(&format!("  {tag:<24} {line}\n"));
    }
    out
}

/// Per-task pass-rate and efficiency deltas over the shared tasks.
fn task_block(shared_b: &[TaskRecord], c_by_id: &BTreeMap<&str, &TaskRecord>) -> String {
    let mut out = String::from("\nper task:\n");
    for tb in shared_b {
        let tc = c_by_id[tb.id.as_str()];
        let (rb, rc) = (pass_rate(tb), pass_rate(tc));
        let eb = Efficiency::over(&tb.runs);
        let ec = Efficiency::over(&tc.runs);
        out.push_str(&format!(
            "  {:<24} pass {:.2} → {:.2} (Δ {:+.2})  wall/run {:.1}s → {:.1}s  \
             attempts/run {} → {}  tools/run {} → {}\n",
            tb.id,
            rb,
            rc,
            rc - rb,
            eb.wall_mean_s(),
            ec.wall_mean_s(),
            report::opt_mean(eb.attempts_mean()),
            report::opt_mean(ec.attempts_mean()),
            report::opt_mean(eb.tools_mean()),
            report::opt_mean(ec.tools_mean()),
        ));
    }
    out
}

/// Name every task present on `side` alone (never silently dropped).
fn unmatched_block(
    side: &str,
    tasks: &[TaskRecord],
    other: &BTreeMap<&str, &TaskRecord>,
) -> String {
    let only: Vec<&str> = tasks
        .iter()
        .filter(|t| !other.contains_key(t.id.as_str()))
        .map(|t| t.id.as_str())
        .collect();
    if only.is_empty() {
        String::new()
    } else {
        format!(
            "\nonly in {side} (excluded from deltas): {}\n",
            only.join(", ")
        )
    }
}

/// One task's per-run pass rate (0 over zero runs).
fn pass_rate(t: &TaskRecord) -> f64 {
    if t.runs.is_empty() {
        return 0.0;
    }
    t.runs.iter().filter(|r| r.pass).count() as f64 / t.runs.len() as f64
}

/// `Δ` between two optional means; `—` unless both sides reported.
fn opt_delta(b: Option<f64>, c: Option<f64>) -> String {
    match (b, c) {
        (Some(b), Some(c)) => format!("{:+.1}", c - b),
        _ => "—".to_string(),
    }
}

/// `b → c (Δ d)` for one usage counter, `—` where unreported.
fn counter_pair(b: Option<u64>, c: Option<u64>) -> String {
    let delta = match (b, c) {
        (Some(b), Some(c)) => format!("{:+}", c as i128 - b as i128),
        _ => "—".to_string(),
    };
    format!(
        "{} → {} (Δ {})",
        report::opt_count(b),
        report::opt_count(c),
        delta
    )
}

/// Re-indent a block two spaces for nesting under a side heading.
fn indent(block: &str) -> String {
    block
        .lines()
        .map(|l| {
            if l.is_empty() {
                String::from("\n")
            } else {
                format!("  {l}\n")
            }
        })
        .collect()
}
