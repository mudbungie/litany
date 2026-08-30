//! The workspace guards (ARCH §2.2, §2.3): *is this a workspace*, *does
//! this agent exist*, *does this ref exist*, *does this config lineage
//! exist*. Split from [`super::tests`] for the per-file line cap; the
//! shared helpers live there.

use super::fixture::{spawn_agent, workspace};
use super::tests::{default_ref, git, head};
use super::*;
use crate::template::GitRunner;
use tempfile::TempDir;

#[test]
fn require_accepts_a_current_workspace() {
    let (_h, ws) = workspace();
    require(&ws).unwrap();
}

#[test]
fn require_refuses_the_retired_layout_with_an_actionable_error() {
    let holder = TempDir::new().unwrap();
    let old = holder.path().join("conv");
    std::fs::create_dir_all(old.join("root/.git")).unwrap();
    std::fs::write(old.join("providers.yaml"), "roles: {}\n").unwrap();
    let err = require(&old).unwrap_err();
    let msg = err.to_string();
    // The refusal names what was found and what the current layout is
    // (pre-v1 clean break, §10) — actionable, not just "no".
    assert!(matches!(err, LayoutError::OldLayout(_)), "{msg}");
    assert!(msg.contains("retired per-conversation layout"), "{msg}");
    assert!(msg.contains("repo.git"), "{msg}");
    assert!(msg.contains("litany new"), "{msg}");
}

#[test]
fn require_refuses_a_non_workspace() {
    let holder = TempDir::new().unwrap();
    let err = require(holder.path()).unwrap_err();
    assert!(matches!(err, LayoutError::NotAWorkspace(_)));
    assert!(err.to_string().contains("litany new"));
}

#[test]
fn require_ref_admits_every_legal_fork_point_and_declines_the_rest() {
    let (_h, ws) = workspace();
    let why = "testing";
    // Any ref is a legal fork point (§2.3): a config branch, an agent
    // branch, and any commit of either — all three answer the guard.
    spawn_agent(&ws, "20260101-r1", &default_ref());
    assert!(require_ref(&ws, &default_ref(), why, &git()).is_ok());
    assert!(require_ref(&ws, &agent_ref("20260101-r1"), why, &git()).is_ok());
    assert!(require_ref(&ws, &head(&ws, &default_ref()), why, &git()).is_ok());
    let err = require_ref(&ws, "config/nope", why, &git()).unwrap_err();
    let msg = err.to_string();
    assert!(msg.contains("no ref or commit \"config/nope\""), "{msg}");
    assert!(msg.contains(why), "{msg}");
    // A name that resolves to a non-commit object is declined by the
    // same guard: a start forks off a commit.
    let tree = git()
        .run_capture(
            &repo_git(&ws),
            &["rev-parse", &format!("{}^{{tree}}", default_ref())],
        )
        .unwrap();
    assert!(require_ref(&ws, &tree, why, &git()).is_err());
}

#[test]
fn require_lineage_admits_the_default_and_names_the_pool_it_declines_from() {
    let (_h, ws) = workspace();
    assert!(require_lineage(&ws, DEFAULT_CONFIG_NAME, &git()).is_ok());
    let err = require_lineage(&ws, "strict", &git()).unwrap_err();
    let msg = err.to_string();
    assert!(msg.contains("no config lineage \"strict\""), "{msg}");
    assert!(msg.contains("existing lineages: default"), "{msg}");
    // The query's own failure is the other arm of the same decline.
    let absent = TempDir::new().unwrap();
    assert!(matches!(
        require_lineage(absent.path(), "default", &git()).unwrap_err(),
        UnknownLineage::Git(_)
    ));
}
