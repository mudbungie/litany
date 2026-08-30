//! Coverage for experiment resolution (ARCH §9.3).

use agent_eval::experiment::{self, ExperimentError};

#[test]
fn resolves_present_experiment() {
    let d = tempfile::tempdir().unwrap();
    let dir = d.path().join("baseline");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("workflow.yaml"), "events: {}\n").unwrap();

    let exp = experiment::resolve("baseline", d.path()).unwrap();
    assert_eq!(exp.name, "baseline");
    // Canonicalized: the path rides to the driver as LITANY_EXPERIMENT,
    // whose cwd is the per-run workdir — it must be absolute.
    assert_eq!(
        exp.workflow,
        dir.join("workflow.yaml").canonicalize().unwrap()
    );
    assert!(exp.workflow.is_absolute());
}

#[test]
fn a_directory_named_workflow_yaml_is_missing() {
    let d = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(d.path().join("odd/workflow.yaml")).unwrap();
    let err = experiment::resolve("odd", d.path()).unwrap_err();
    assert!(matches!(err, ExperimentError::Missing { .. }));
}

#[test]
fn missing_experiment_errors() {
    let d = tempfile::tempdir().unwrap();
    let err = experiment::resolve("ghost", d.path()).unwrap_err();
    assert!(matches!(err, ExperimentError::Missing { .. }));
    assert!(err.to_string().contains("ghost"));
    assert!(err.to_string().contains("workflow.yaml"));
}

#[test]
fn the_shipped_baseline_resolves_to_the_template_itself() {
    // The repo ships experiments/baseline/workflow.yaml (§9.3) — and it
    // is the template, not a copy of it: an experiment is a diff against
    // the shipped default, so the baseline's diff is empty and the path
    // is a symlink to `template/workflow.yaml`. Resolution is unaffected
    // (no fallback, no special case); canonicalizing both names proves
    // there is one file, so the baseline cannot drift from the default.
    let repo = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let exp = experiment::resolve("baseline", &repo.join("experiments")).unwrap();
    assert!(exp.workflow.is_file());
    assert_eq!(
        exp.workflow.canonicalize().unwrap(),
        repo.join("template/workflow.yaml").canonicalize().unwrap(),
    );
}
