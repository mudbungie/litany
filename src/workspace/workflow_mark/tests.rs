//! The workflow mark against a real workspace (ARCH §6).

use super::{clear, read, workflow_ref, write};
use crate::template::{GitRunner, RealGit};
use crate::workspace::{config_ref, fixture, repo_git};
use std::path::{Path, PathBuf};

/// A workspace with one root agent — the shape a switch addresses.
fn agent() -> (tempfile::TempDir, PathBuf) {
    let (holder, ws) = fixture::workspace();
    fixture::spawn_root(&ws, "a");
    (holder, ws)
}

/// The head of `config/default` — the ordinary mark target.
fn config_head(ws: &Path) -> String {
    RealGit::new()
        .run_capture(&repo_git(ws), &["rev-parse", &config_ref("default")])
        .unwrap()
        .trim()
        .to_string()
}

#[test]
fn the_mark_ref_lives_in_the_shared_per_agent_mark_namespace() {
    // §9.2's retention delete enumerates `refs/litany/`, so a mark that
    // spelled its own root would outlive the agent it belongs to.
    assert_eq!(workflow_ref("a-b"), "refs/litany/workflow/a-b");
}

#[test]
fn an_unset_mark_reads_as_none_which_is_every_agents_ordinary_state() {
    let (_h, ws) = agent();
    assert_eq!(read(&ws, "a", &RealGit::new()), None);
}

#[test]
fn a_written_mark_reads_back_the_workflow_source_commit() {
    let (_h, ws) = agent();
    let git = RealGit::new();
    let head = config_head(&ws);
    write(&ws, "a", &head, &git).unwrap();
    assert_eq!(read(&ws, "a", &git), Some(head));
}

#[test]
fn a_second_mark_wins_so_switching_again_is_just_marking_again() {
    let (_h, ws) = agent();
    let git = RealGit::new();
    let first = config_head(&ws);
    fixture::amend_config(
        &ws,
        &[(
            "workflow.yaml",
            "events: {}\nretry:\n  max_attempts: 2\n  backoff: exponential\n",
        )],
    );
    let second = config_head(&ws);
    assert_ne!(first, second);
    write(&ws, "a", &first, &git).unwrap();
    write(&ws, "a", &second, &git).unwrap();
    assert_eq!(read(&ws, "a", &git), Some(second));
}

#[test]
fn the_mark_is_per_agent_so_one_switch_never_moves_another() {
    let (_h, ws) = agent();
    let git = RealGit::new();
    fixture::spawn_root(&ws, "b");
    write(&ws, "a", &config_head(&ws), &git).unwrap();
    assert_eq!(read(&ws, "b", &git), None);
}

#[test]
fn clearing_returns_the_agent_to_its_governing_configs_workflow() {
    let (_h, ws) = agent();
    let git = RealGit::new();
    write(&ws, "a", &config_head(&ws), &git).unwrap();
    clear(&ws, "a", &git).unwrap();
    assert_eq!(read(&ws, "a", &git), None);
}

#[test]
fn a_mark_naming_a_ref_stores_the_commit_it_resolves_to() {
    // The verb hands `config/<name>`'s head as a commit; the read
    // resolves a commit-ish either way, so a lineage that advances after
    // the mark still answers the commit that was marked — chosen
    // immutability, the same property the config freeze buys (§2.2).
    let (_h, ws) = agent();
    let git = RealGit::new();
    let head = config_head(&ws);
    write(&ws, "a", &config_ref("default"), &git).unwrap();
    fixture::amend_config(
        &ws,
        &[(
            "workflow.yaml",
            "events: {}\nretry:\n  max_attempts: 2\n  backoff: exponential\n",
        )],
    );
    assert_eq!(read(&ws, "a", &git), Some(head));
}

#[test]
fn a_workspace_with_no_repo_reads_as_none_rather_than_failing_a_step() {
    let holder = tempfile::TempDir::new().unwrap();
    assert_eq!(read(holder.path(), "a", &RealGit::new()), None);
}

#[test]
fn a_write_into_a_workspace_with_no_repo_surfaces_the_failure() {
    let holder = tempfile::TempDir::new().unwrap();
    assert!(write(holder.path(), "a", "HEAD", &RealGit::new()).is_err());
}

#[test]
fn clearing_a_mark_that_was_never_set_is_a_clean_no_op() {
    // `update-ref -d` of an absent ref is git's own no-op — the general
    // path with empty inputs, not a case to branch on.
    let (_h, ws) = agent();
    clear(&ws, "a", &RealGit::new()).unwrap();
    assert_eq!(read(&ws, "a", &RealGit::new()), None);
}
