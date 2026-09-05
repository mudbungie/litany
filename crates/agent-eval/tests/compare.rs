//! Coverage for the baseline → candidate comparison (bl-36fa, ARCH
//! §9.3): per-task, per-category, and total deltas; the shared-set
//! rule; and missing-metric handling (`Δ —`, never a fabricated zero).

use agent_eval::compare;
use agent_eval::metrics::RunMetrics;
use agent_eval::record::{Provenance, Record, RunRecord, TaskRecord};

fn provenance(driver: &str) -> Provenance {
    Provenance {
        experiment: "baseline".to_string(),
        workflow: "/x/workflow.yaml".to_string(),
        suite: "tests/suite".to_string(),
        suite_revision: Some("abc123".to_string()),
        fixture_digest: Some("00ff".to_string()),
        driver: driver.to_string(),
        driver_version: Some(format!("{driver} 1.0")),
        runs_per_task: 2,
    }
}

fn run(pass: bool, wall_ms: u64, metrics: Option<RunMetrics>) -> RunRecord {
    RunRecord {
        pass,
        wall_ms,
        metrics,
    }
}

fn metrics(attempts: u64, tools: u64, input: Option<u64>) -> RunMetrics {
    RunMetrics {
        attempts,
        tool_invocations: tools,
        input_tokens: input,
        output_tokens: input,
        cache_read_tokens: None,
        cache_write_tokens: None,
        models: vec!["m1".to_string()],
        providers: vec!["acme".to_string()],
    }
}

fn task(id: &str, cat: &str, runs: Vec<RunRecord>) -> TaskRecord {
    TaskRecord {
        id: id.to_string(),
        categories: vec![cat.to_string()],
        runs,
    }
}

#[test]
fn compare_reports_deltas_at_every_grain() {
    let baseline = Record {
        provenance: provenance("driver-a"),
        tasks: vec![
            task(
                "shared",
                "early_termination",
                vec![
                    run(true, 2000, Some(metrics(4, 10, Some(100)))),
                    run(false, 2000, Some(metrics(4, 10, Some(100)))),
                ],
            ),
            task("only-base", "scope_reduction", vec![run(true, 100, None)]),
        ],
    };
    let candidate = Record {
        provenance: provenance("driver-b"),
        tasks: vec![
            task(
                "shared",
                "early_termination",
                vec![
                    run(true, 1000, Some(metrics(2, 6, Some(80)))),
                    run(true, 1000, Some(metrics(2, 6, Some(80)))),
                ],
            ),
            task("only-cand", "skipped_tests", vec![run(false, 100, None)]),
        ],
    };
    let text = compare::render(&baseline, &candidate);

    assert!(text.contains("comparison over 1 shared task(s)"));
    // Both sides' reproducibility inputs are printed.
    assert!(text.contains("driver: driver-a (driver-a 1.0)"));
    assert!(text.contains("driver: driver-b (driver-b 1.0)"));
    // Total quality: 50% → 100% over the shared task.
    assert!(text.contains("Δ +50.0"));
    assert!(text.contains("pass@5: 100.0% → 100.0%   Δ +0.0"));
    // Total efficiency: wall 2.0s → 1.0s, attempts 4 → 2, tools 10 → 6.
    assert!(text.contains("outer wall/run: 2.0s → 1.0s   Δ -1.0s"));
    assert!(text.contains("attempts/run: 4.0 → 2.0   Δ -2.0"));
    assert!(text.contains("tool invocations/run: 10.0 → 6.0   Δ -4.0"));
    // Usage: reported counters get numeric deltas; unreported stay —.
    assert!(text.contains("input 200 → 160 (Δ -40)"));
    assert!(text.contains("cache_read — → — (Δ —)"));
    // Per category and per task.
    assert!(text.contains("per category:"));
    assert!(text.contains("early_termination"));
    assert!(text.contains("pass@1 50.0% → 100.0% (Δ +50.0)"));
    assert!(text.contains("per task:"));
    assert!(text.contains("shared"));
    assert!(text.contains("pass 0.50 → 1.00 (Δ +0.50)"));
    // Unshared tasks are named, not silently dropped.
    assert!(text.contains("only in baseline (excluded from deltas): only-base"));
    assert!(text.contains("only in candidate (excluded from deltas): only-cand"));
}

#[test]
fn missing_metrics_on_one_side_yield_no_delta() {
    // Candidate never disclosed a workspace (e.g. a foreign driver with
    // no litany report): quality still compares; efficiency deltas are
    // — on every derived metric.
    let baseline = Record {
        provenance: provenance("driver-a"),
        tasks: vec![task(
            "t",
            "early_termination",
            vec![run(true, 1000, Some(metrics(1, 1, Some(10))))],
        )],
    };
    let candidate = Record {
        provenance: provenance("driver-b"),
        tasks: vec![task("t", "hallucinated_apis", vec![run(true, 500, None)])],
    };
    let text = compare::render(&baseline, &candidate);
    assert!(text.contains("attempts/run: 1.0 → —   Δ —"));
    assert!(text.contains("tool invocations/run: 1.0 → —   Δ —"));
    assert!(text.contains("input 10 → — (Δ —)"));
    // The categories differ per side: each is one-sided here.
    assert!(text.contains("early_termination"));
    assert!(text.contains("hallucinated_apis"));
    assert!(text.contains("present on one side only — Δ —"));
    // No unmatched-task lines: the shared set is total.
    assert!(!text.contains("only in"));
}

#[test]
fn zero_run_tasks_compare_without_dividing() {
    let empty = |driver: &str| Record {
        provenance: provenance(driver),
        tasks: vec![task("t", "early_termination", vec![])],
    };
    let text = compare::render(&empty("a"), &empty("b"));
    assert!(text.contains("pass 0.00 → 0.00 (Δ +0.00)"));
}

#[test]
fn zero_run_tasks_render_no_nan_anywhere() {
    // The total block aggregates over the shared set through
    // `stats::summarize`; a zero-run task must read as unmeasured, not
    // as a 0/0 rate rendered "NaN%".
    let empty = |driver: &str| Record {
        provenance: provenance(driver),
        tasks: vec![task("t", "early_termination", vec![])],
    };
    let text = compare::render(&empty("a"), &empty("b"));
    assert!(!text.contains("NaN"), "NaN leaked into the report:\n{text}");
    assert!(text.contains("pass@1: 0.0% [0.0%, 0.0%] → 0.0% [0.0%, 0.0%]"));
}

#[test]
fn the_header_names_both_experiments() {
    let baseline = Record {
        provenance: provenance("driver-a"),
        tasks: vec![task("t", "early_termination", vec![run(true, 100, None)])],
    };
    let mut candidate = Record {
        provenance: provenance("driver-a"),
        tasks: vec![task("t", "early_termination", vec![run(true, 100, None)])],
    };
    candidate.provenance.experiment = "single-attempt".to_string();
    let text = compare::render(&baseline, &candidate);
    assert!(text.contains("baseline baseline → candidate single-attempt"));
}

#[test]
fn held_controls_render_as_held_across_experiments() {
    // Same controls, different experiment (the treatment): held — the
    // one-invocation comparison (bl-f838) always reads this way.
    let baseline = Record {
        provenance: provenance("driver-a"),
        tasks: vec![task(
            "t",
            "early_termination",
            vec![run(true, 100, Some(metrics(1, 1, Some(1))))],
        )],
    };
    let mut candidate = baseline.clone();
    candidate.provenance.experiment = "variant".to_string();
    candidate.provenance.workflow = "/y/workflow.yaml".to_string();
    let text = compare::render(&baseline, &candidate);
    assert!(text.contains("controls: held — the experiment is the only declared difference"));
    assert!(!text.contains("controls differ"));
}

#[test]
fn differing_controls_and_observed_models_are_named() {
    let baseline = Record {
        provenance: provenance("driver-a"),
        tasks: vec![task(
            "t",
            "early_termination",
            vec![run(true, 100, Some(metrics(1, 1, Some(1))))],
        )],
    };
    let mut candidate = baseline.clone();
    candidate.provenance.driver = "driver-b".to_string();
    candidate.provenance.driver_version = Some("driver-b 1.0".to_string());
    candidate.tasks[0].runs[0].metrics.as_mut().unwrap().models = vec!["m2".to_string()];
    let text = compare::render(&baseline, &candidate);
    assert!(text.contains(
        "controls differ: driver, driver version, observed models — \
         the deltas below compare more than the experiment"
    ));
}

#[test]
fn an_unreported_model_set_is_never_called_a_difference() {
    // Candidate disclosed no workspace: its observed set is empty —
    // unknown, not different (missing is never treated as a value).
    let baseline = Record {
        provenance: provenance("driver-a"),
        tasks: vec![task(
            "t",
            "early_termination",
            vec![run(true, 100, Some(metrics(1, 1, Some(1))))],
        )],
    };
    let mut candidate = baseline.clone();
    candidate.tasks[0].runs[0].metrics = None;
    let text = compare::render(&baseline, &candidate);
    assert!(text.contains("controls: held"));
}

#[test]
fn the_report_answers_whether_the_pass_at_1_delta_is_real() {
    // bl-a35e: the block above it renders each side's own Wilson
    // interval and the difference, which is *how big* and never
    // *whether*. Six tasks all improving is the smallest unanimous
    // sample whose exact two-sided p (0.03125) clears 0.05.
    let ids = ["a", "b", "c", "d", "e", "f"];
    let arm = |pass: bool| {
        ids.iter()
            .map(|id| task(id, "early_termination", vec![run(pass, 1000, None)]))
            .collect::<Vec<_>>()
    };
    let out = compare::render(
        &Record {
            provenance: provenance("d"),
            tasks: arm(false),
        },
        &Record {
            provenance: provenance("d"),
            tasks: arm(true),
        },
    );
    assert!(out.contains("pass@1 significance: p = 0.0312"), "{out}");
    assert!(out.contains("(significant at alpha = 0.05)"), "{out}");
    assert!(
        out.contains("6 better, 0 worse, 0 tied of 6 shared task(s)"),
        "{out}"
    );
    assert!(out.contains("exact two-sided sign test"), "{out}");
}

#[test]
fn a_comparison_where_nothing_moved_reports_no_verdict() {
    // An empty null distribution is the absence of an answer, and the
    // report says so rather than printing a measured-looking p = 1.
    let arm = || vec![task("a", "early_termination", vec![run(true, 1000, None)])];
    let out = compare::render(
        &Record {
            provenance: provenance("d"),
            tasks: arm(),
        },
        &Record {
            provenance: provenance("d"),
            tasks: arm(),
        },
    );
    assert!(
        out.contains("pass@1 significance: no task moved — no verdict"),
        "{out}"
    );
    assert!(
        out.contains("0 better, 0 worse, 1 tied of 1 shared task(s)"),
        "{out}"
    );
}
