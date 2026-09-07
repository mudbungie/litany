//! [`super::validate`] and [`super::against`] against real config
//! commits (ARCH §4.3): the two absences that make a role invalid, in
//! the voice each front door speaks them (`docs/PRINCIPLES.md`).

use super::*;
use crate::template::RealGit;
use crate::workspace::fixture;

fn git() -> RealGit {
    RealGit::new()
}

/// The default scaffold lists `worker` with `souls/worker.md`, so a
/// worker off a fresh root validates.
#[test]
fn a_config_role_with_its_soul_is_valid() {
    let (_h, ws) = fixture::workspace();
    fixture::spawn_root(&ws, "p1");
    validate(&ws, "p1", None, "worker", &git()).unwrap();
}

/// A third role the config defines — the v0.7 verifier, zero code —
/// validates exactly like the template roles.
#[test]
fn a_third_config_role_is_valid_zero_code() {
    let (_h, ws) = fixture::workspace();
    let yaml = "roles:\n  worker:\n    provider: anthropic\n    model: sonnet\n  \
                verifier:\n    provider: anthropic\n    model: sonnet\n";
    fixture::amend_config(
        &ws,
        &[("providers.yaml", yaml), ("souls/verifier.md", "v\n")],
    );
    fixture::spawn_root(&ws, "p9");
    validate(&ws, "p9", None, "verifier", &git()).unwrap();
}

#[test]
fn a_role_absent_from_providers_is_role_missing() {
    let (_h, ws) = fixture::workspace();
    fixture::spawn_root(&ws, "p1");
    let err = validate(&ws, "p1", None, "ghost", &git()).unwrap_err();
    match &err {
        Invalid::RoleMissing {
            role,
            subject,
            defined,
        } => {
            assert_eq!(role, "ghost");
            assert_eq!(subject, "a child of agent \"p1\"");
            assert_eq!(defined, "compactor, planner, reviewer, worker");
        }
        other => panic!("expected RoleMissing, got {other:?}"),
    }
    // bl-c89b: the product's voice — no commit sha, no `<sha>:path`
    // git-show form — and it names the pool that IS defined.
    assert_eq!(
        err.to_string(),
        "role \"ghost\" is not defined in the providers.yaml that will govern a child \
         of agent \"p1\" \
         — defined roles: compactor, planner, reviewer, worker"
    );
}

#[test]
fn a_role_listed_without_a_soul_is_soul_missing() {
    let (_h, ws) = fixture::workspace();
    let yaml = "roles:\n  verifier:\n    provider: anthropic\n    model: sonnet\n";
    fixture::amend_config(&ws, &[("providers.yaml", yaml)]);
    fixture::spawn_root(&ws, "p9");
    let err = validate(&ws, "p9", None, "verifier", &git()).unwrap_err();
    match &err {
        Invalid::SoulMissing { role, subject } => {
            assert_eq!(role, "verifier");
            assert_eq!(subject, "a child of agent \"p9\"");
        }
        other => panic!("expected SoulMissing, got {other:?}"),
    }
    assert_eq!(
        err.to_string(),
        "role \"verifier\" is defined but its soul souls/verifier.md is missing from \
         the config that will govern a child of agent \"p9\" — a role is its \
         `roles:` entry and its \
         soul (ARCH §4.3)"
    );
}

#[test]
fn a_legacy_providers_yaml_is_config_error() {
    let (_h, ws) = fixture::workspace();
    fixture::amend_config(&ws, &[("providers.yaml", "providers: {}\n")]);
    fixture::spawn_root(&ws, "p9");
    let err = validate(&ws, "p9", None, "worker", &git()).unwrap_err();
    assert!(matches!(err, Invalid::Config(_)), "{err:?}");
    assert!(err.to_string().starts_with("providers.yaml:"));
}

#[test]
fn a_non_workspace_repo_is_a_governing_error() {
    let holder = tempfile::TempDir::new().unwrap();
    let err = validate(holder.path(), "p1", None, "worker", &git()).unwrap_err();
    match &err {
        Invalid::Governing { subject, .. } => {
            assert_eq!(subject, "a child of agent \"p1\"");
        }
        other => panic!("expected Governing, got {other:?}"),
    }
    assert!(
        err.to_string()
            .contains("governing config for a child of agent \"p1\"")
    );
}

#[test]
fn a_commit_whose_providers_yaml_cannot_be_read_is_a_governing_error() {
    // [`against`]'s own read arm: the caller hands a commit, so nothing
    // derives one — a commit whose tree has no `providers.yaml` (a
    // rewound lineage, a sha from another store) declines here rather
    // than answering "role undefined" about a file it never saw.
    let (_h, ws) = fixture::workspace();
    let err = against(&ws, "0000000", "agent \"a\"", "worker", &git()).unwrap_err();
    match &err {
        Invalid::Governing { subject, .. } => assert_eq!(subject, "agent \"a\""),
        other => panic!("expected Governing, got {other:?}"),
    }
    assert!(
        err.to_string().contains("governing config for agent \"a\""),
        "{err}"
    );
}

#[test]
fn the_subject_is_the_callers_phrase_so_a_retarget_speaks_of_the_agent() {
    // The decline names what the commit is about to govern (bl-946c): a
    // dispatch says *a child of* the agent, a retarget says the agent.
    let (_h, ws) = fixture::workspace();
    let commit = crate::workspace::current_config::current_config(
        &ws,
        &crate::workspace::config_ref("default"),
        &git(),
    )
    .unwrap()
    .commit()
    .to_string();
    let err = against(&ws, &commit, "agent \"a\"", "ghost", &git()).unwrap_err();
    assert_eq!(
        err.to_string(),
        "role \"ghost\" is not defined in the providers.yaml that will govern agent \"a\" \
         — defined roles: compactor, planner, reviewer, worker"
    );
}
