//! The retarget mark against a real workspace (ARCH §2.2).

use super::{clear, read, retarget_ref, write};
use crate::template::{GitRunner, RealGit};
use crate::workspace::{config_ref, fixture, repo_git};
use std::path::{Path, PathBuf};

/// A workspace with one root agent — the shape a retarget addresses.
fn agent() -> (tempfile::TempDir, PathBuf) {
    let (holder, ws) = fixture::workspace();
    fixture::spawn_root(&ws, "a");
    (holder, ws)
}

/// The head of `config/default` — the ordinary retarget target.
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
    assert_eq!(retarget_ref("a-b"), "refs/litany/retarget/a-b");
}

#[test]
fn an_unset_mark_reads_as_none_which_is_every_agents_ordinary_state() {
    let (_h, ws) = agent();
    assert_eq!(read(&ws, "a", &RealGit::new()), None);
}

#[test]
fn a_written_mark_reads_back_the_target_config_commit() {
    let (_h, ws) = agent();
    let git = RealGit::new();
    let head = config_head(&ws);
    write(&ws, "a", &head, &git).unwrap();
    assert_eq!(read(&ws, "a", &git), Some(head));
}

#[test]
fn a_second_mark_wins_so_an_operator_may_change_their_mind() {
    let (_h, ws) = agent();
    let git = RealGit::new();
    let first = config_head(&ws);
    fixture::amend_config(&ws, &[("souls/worker.md", "an amended soul\n")]);
    let second = config_head(&ws);
    assert_ne!(first, second);
    write(&ws, "a", &first, &git).unwrap();
    write(&ws, "a", &second, &git).unwrap();
    assert_eq!(read(&ws, "a", &git), Some(second));
}

#[test]
fn the_mark_is_per_agent_so_one_retarget_never_moves_another() {
    let (_h, ws) = agent();
    let git = RealGit::new();
    fixture::spawn_root(&ws, "b");
    write(&ws, "a", &config_head(&ws), &git).unwrap();
    assert_eq!(read(&ws, "b", &git), None);
}

#[test]
fn clearing_consumes_the_mark_so_the_next_boundary_does_not_re_ask() {
    let (_h, ws) = agent();
    let git = RealGit::new();
    write(&ws, "a", &config_head(&ws), &git).unwrap();
    clear(&ws, "a", &git).unwrap();
    assert_eq!(read(&ws, "a", &git), None);
}

#[test]
fn a_mark_naming_a_ref_stores_the_commit_it_resolves_to() {
    // The verb hands `config/<name>`; the mark is a commit-ish either
    // way, and the read resolves it — so a lineage that advances after
    // the mark still lands the commit that was marked.
    let (_h, ws) = agent();
    let git = RealGit::new();
    let head = config_head(&ws);
    write(&ws, "a", &config_ref("default"), &git).unwrap();
    fixture::amend_config(&ws, &[("souls/worker.md", "an amended soul\n")]);
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
    // `update-ref -d` of an absent ref is git's own no-op, so the
    // executor's consume needs no "was it there" guard — the general path
    // with empty inputs, not a case to branch on.
    let (_h, ws) = agent();
    clear(&ws, "a", &RealGit::new()).unwrap();
    assert_eq!(read(&ws, "a", &RealGit::new()), None);
}
