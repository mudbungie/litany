//! Coverage for the evaluation record (bl-36fa): save/load round-trip,
//! load failures, the stats projection, and the derived observation
//! sets.

use agent_eval::experiment::Experiment;
use agent_eval::metrics::RunMetrics;
use agent_eval::record::{self, Controls, Provenance, Record, RecordError, RunRecord, TaskRecord};

pub fn provenance() -> Provenance {
    Provenance {
        experiment: "baseline".to_string(),
        workflow: "/x/workflow.yaml".to_string(),
        suite: "tests/suite".to_string(),
        suite_revision: Some("abc123".to_string()),
        fixture_digest: Some("00ff".to_string()),
        driver: "fake-driver".to_string(),
        driver_version: Some("fake-driver 1.0".to_string()),
        runs_per_task: 2,
    }
}

fn metrics(models: &[&str], providers: &[&str]) -> RunMetrics {
    RunMetrics {
        attempts: 1,
        tool_invocations: 2,
        input_tokens: Some(10),
        output_tokens: Some(5),
        cache_read_tokens: None,
        cache_write_tokens: None,
        models: models.iter().map(|s| s.to_string()).collect(),
        providers: providers.iter().map(|s| s.to_string()).collect(),
    }
}

fn record() -> Record {
    Record {
        provenance: provenance(),
        tasks: vec![
            TaskRecord {
                id: "a".to_string(),
                categories: vec!["early_termination".to_string()],
                runs: vec![
                    RunRecord {
                        pass: true,
                        wall_ms: 1500,
                        metrics: Some(metrics(&["m1"], &["acme"])),
                    },
                    RunRecord {
                        pass: false,
                        wall_ms: 500,
                        metrics: Some(metrics(&["m2", "m1"], &["other"])),
                    },
                ],
            },
            TaskRecord {
                id: "b".to_string(),
                categories: vec!["scope_reduction".to_string()],
                runs: vec![
                    RunRecord {
                        pass: false,
                        wall_ms: 0,
                        metrics: None,
                    },
                    RunRecord {
                        pass: false,
                        wall_ms: 100,
                        metrics: None,
                    },
                ],
            },
        ],
    }
}

#[test]
fn save_then_load_round_trips() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("record.json");
    let r = record();
    r.save(&path).unwrap();
    let loaded = Record::load(&path).unwrap();
    assert_eq!(loaded, r);
}

#[test]
fn load_names_the_failure() {
    let dir = tempfile::tempdir().unwrap();
    let missing = Record::load(&dir.path().join("nope.json")).unwrap_err();
    assert!(matches!(missing, RecordError::Read { .. }));
    assert!(missing.to_string().contains("read record"));

    let bad = dir.path().join("bad.json");
    std::fs::write(&bad, "not json").unwrap();
    let parse = Record::load(&bad).unwrap_err();
    assert!(matches!(parse, RecordError::Parse { .. }));
    assert!(parse.to_string().contains("parse record"));
}

#[test]
fn task_results_project_pass_fail_for_stats() {
    let results = record().task_results();
    assert_eq!(results.len(), 2);
    assert_eq!(results[0].id, "a");
    assert_eq!(results[0].outcomes, vec![true, false]);
    assert_eq!(results[0].categories, vec!["early_termination".to_string()]);
    assert_eq!(results[1].outcomes, vec![false, false]);
}

#[test]
fn observed_sets_are_sorted_unions_over_disclosed_runs() {
    let r = record();
    assert_eq!(
        r.observed_models(),
        vec!["m1".to_string(), "m2".to_string()]
    );
    assert_eq!(
        r.observed_providers(),
        vec!["acme".to_string(), "other".to_string()]
    );
}

fn controls() -> Controls {
    Controls {
        suite: "tests/suite".to_string(),
        suite_revision: Some("abc123".to_string()),
        fixture_digest: Some("00ff".to_string()),
        driver: "fake-driver".to_string(),
        driver_version: Some("fake-driver 1.0".to_string()),
        runs_per_task: 2,
    }
}

#[test]
fn controls_plus_an_experiment_is_a_provenance() {
    let exp = Experiment {
        name: "baseline".to_string(),
        workflow: "/x/workflow.yaml".into(),
    };
    assert_eq!(controls().provenance(&exp), provenance());
}

#[test]
fn controls_diff_is_empty_when_held_even_across_experiments() {
    // The experiment and its workflow are the treatment, never listed.
    let mut variant = provenance();
    variant.experiment = "variant".to_string();
    variant.workflow = "/y/workflow.yaml".to_string();
    assert!(record::controls_diff(&provenance(), &variant).is_empty());
}

#[test]
fn controls_diff_names_every_differing_control() {
    let mut other = provenance();
    other.suite = "elsewhere".to_string();
    other.suite_revision = None;
    other.fixture_digest = None;
    other.driver = "other-driver".to_string();
    other.driver_version = None;
    other.runs_per_task = 9;
    assert_eq!(
        record::controls_diff(&provenance(), &other),
        vec![
            "suite",
            "suite revision",
            "starting fixture",
            "driver",
            "driver version",
            "runs/task",
        ]
    );
}

#[test]
fn save_all_writes_one_record_as_a_file() {
    // The bl-36fa single-record contract, byte-for-byte: `path` is the
    // file itself, not a directory.
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("record.json");
    record::save_all(std::slice::from_ref(&record()), &path).unwrap();
    assert_eq!(Record::load(&path).unwrap(), record());
}

#[test]
fn save_all_writes_several_records_as_a_directory() {
    let dir = tempfile::tempdir().unwrap();
    let out = dir.path().join("records");
    let mut variant = record();
    variant.provenance.experiment = "variant".to_string();
    record::save_all(&[record(), variant.clone()], &out).unwrap();
    assert_eq!(Record::load(&out.join("baseline.json")).unwrap(), record());
    assert_eq!(Record::load(&out.join("variant.json")).unwrap(), variant);
}

#[test]
fn save_all_of_nothing_is_an_empty_directory() {
    let dir = tempfile::tempdir().unwrap();
    let out = dir.path().join("records");
    record::save_all(&[], &out).unwrap();
    assert!(out.is_dir());
    assert_eq!(std::fs::read_dir(&out).unwrap().count(), 0);
}
