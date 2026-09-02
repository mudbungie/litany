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

#[test]
fn resolve_all_preserves_the_given_order() {
    let d = tempfile::tempdir().unwrap();
    for name in ["baseline", "variant"] {
        let dir = d.path().join(name);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("workflow.yaml"), "events: {}\n").unwrap();
    }
    let configs = vec!["baseline".to_string(), "variant".to_string()];
    let all = experiment::resolve_all(&configs, d.path()).unwrap();
    // Order is the comparison's meaning: the first is the baseline.
    assert_eq!(all.len(), 2);
    assert_eq!(all[0].name, "baseline");
    assert_eq!(all[1].name, "variant");
}

#[test]
fn resolve_all_refuses_a_repeated_name() {
    let d = tempfile::tempdir().unwrap();
    let dir = d.path().join("baseline");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("workflow.yaml"), "events: {}\n").unwrap();
    let configs = vec!["baseline".to_string(), "baseline".to_string()];
    let err = experiment::resolve_all(&configs, d.path()).unwrap_err();
    assert!(matches!(err, ExperimentError::Duplicate { .. }));
    assert!(err.to_string().contains("baseline"));
    assert!(err.to_string().contains("more than once"));
}

#[test]
fn resolve_all_surfaces_a_missing_variant() {
    let d = tempfile::tempdir().unwrap();
    let err = experiment::resolve_all(&["ghost".to_string()], d.path()).unwrap_err();
    assert!(matches!(err, ExperimentError::Missing { .. }));
}
