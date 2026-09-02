//! Coverage for the report renderer (ARCH §9.1, §9.3; bl-36fa): the
//! quality sections, the efficiency aggregates with the missing-≠-zero
//! rendering, and the reproducibility block in both its known and
//! unknown shapes.

use agent_eval::metrics::RunMetrics;
use agent_eval::record::{Provenance, Record, RunRecord, TaskRecord};
use agent_eval::report;

fn run(pass: bool, wall_ms: u64, metrics: Option<RunMetrics>) -> RunRecord {
    RunRecord {
        pass,
        wall_ms,
        metrics,
    }
}

fn task(id: &str, cats: &[&str], runs: Vec<RunRecord>) -> TaskRecord {
    TaskRecord {
        id: id.to_string(),
        categories: cats.iter().map(|s| s.to_string()).collect(),
        runs,
    }
}

fn full_metrics() -> RunMetrics {
    RunMetrics {
        attempts: 2,
        tool_invocations: 4,
        input_tokens: Some(100),
        output_tokens: Some(40),
        cache_read_tokens: Some(10),
        cache_write_tokens: None,
        models: vec!["m1".to_string()],
        providers: vec!["acme".to_string()],
    }
}

fn known_provenance() -> Provenance {
    Provenance {
        experiment: "baseline".to_string(),
        workflow: "/x/workflow.yaml".to_string(),
        suite: "tests/suite".to_string(),
        suite_revision: Some("abc123+dirty".to_string()),
        fixture_digest: Some("00ff".to_string()),
        driver: "fake-driver".to_string(),
        driver_version: Some("fake-driver 1.0".to_string()),
        runs_per_task: 5,
    }
}

#[test]
fn render_includes_all_sections() {
    let record = Record {
        provenance: known_provenance(),
        tasks: vec![
            task(
                "a",
                &["early_termination"],
                vec![
                    run(true, 2000, Some(full_metrics())),
                    run(true, 1000, Some(full_metrics())),
                    run(false, 1000, None),
                    run(false, 1000, None),
                    run(true, 5000, None),
                ],
            ),
            task(
                "b",
                &["scope_reduction"],
                vec![
                    run(false, 0, None),
                    run(false, 0, None),
                    run(false, 0, None),
                    run(false, 0, None),
                    run(false, 0, None),
                ],
            ),
        ],
    };
    let text = report::render(&record);

    assert!(text.contains("experiment: baseline"));
    assert!(text.contains("tasks: 2"));
    assert!(text.contains("runs/task: 5"));
    assert!(text.contains("pass@1 (reliability):"));
    assert!(text.contains("pass@5 (capability):"));
    assert!(text.contains("per category (§9.1):"));
    assert!(text.contains("early_termination"));
    assert!(text.contains("scope_reduction"));
    assert!(text.contains("baseline criterion (§9.1, v0.9)"));
    // pass@1 line carries a percentage and a bracketed interval.
    assert!(text.contains('%'));
    assert!(text.contains('['));

    // Efficiency: wall over all 10 runs (10s/10 = 1.0s); the derived
    // means over the 2 disclosed runs; cache_write never reported.
    assert!(text.contains("runs with workspace metrics: 2/10"));
    assert!(text.contains("outer wall/run: 1.0s"));
    assert!(text.contains("attempts/run: 2.0"));
    assert!(text.contains("tool invocations/run: 4.0"));
    assert!(text.contains("input 200"));
    assert!(text.contains("cache_write —"));

    // Reproducibility: every probed input reported.
    assert!(text.contains("suite: tests/suite @ abc123+dirty"));
    assert!(text.contains("starting fixture: sha256:00ff"));
    assert!(text.contains("experiment: baseline (/x/workflow.yaml)"));
    assert!(text.contains("driver: fake-driver (fake-driver 1.0)"));
    assert!(text.contains("models: m1   providers: acme"));
}

#[test]
fn unknowns_render_as_unknown_never_as_zero() {
    let record = Record {
        provenance: Provenance {
            suite_revision: None,
            fixture_digest: None,
            driver_version: None,
            ..known_provenance()
        },
        tasks: vec![task("a", &["early_termination"], vec![run(false, 0, None)])],
    };
    let text = report::render(&record);
    assert!(text.contains("@ revision unknown"));
    assert!(text.contains("starting fixture: unknown"));
    assert!(text.contains("(version unreported)"));
    assert!(text.contains("models: unreported   providers: unreported"));
    // No run disclosed a workspace: every derived metric is missing.
    assert!(text.contains("runs with workspace metrics: 0/1"));
    assert!(text.contains("attempts/run: —"));
    assert!(text.contains("tool invocations/run: —"));
    assert!(text.contains("input —  output —  cache_read —  cache_write —"));
}

#[test]
fn render_all_of_one_is_the_single_report() {
    let record = Record {
        provenance: known_provenance(),
        tasks: vec![task(
            "t",
            &["early_termination"],
            vec![run(true, 100, None)],
        )],
    };
    assert_eq!(
        report::render_all(std::slice::from_ref(&record)),
        report::render(&record)
    );
}

#[test]
fn render_all_of_many_is_a_comparison_per_candidate() {
    let named = |name: &str| {
        let mut p = known_provenance();
        p.experiment = name.to_string();
        Record {
            provenance: p,
            tasks: vec![task(
                "t",
                &["early_termination"],
                vec![run(true, 100, None)],
            )],
        }
    };
    let records = [named("baseline"), named("alt-a"), named("alt-b")];
    let text = report::render_all(&records);
    // The first record is the baseline of every comparison block.
    assert!(text.contains("baseline baseline → candidate alt-a"));
    assert!(text.contains("baseline baseline → candidate alt-b"));
    assert!(!text.contains("baseline alt-a"));
}

#[test]
fn render_all_of_nothing_is_empty() {
    assert!(report::render_all(&[]).is_empty());
}
