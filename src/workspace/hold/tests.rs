//! The hold mark against a real workspace (ARCH §3.3 *Tool control*).

use super::{Held, clear, hold_ref, read, write};
use crate::template::{GitRunner, RealGit};
use crate::workspace::{fixture, repo_git};
use std::path::PathBuf;

/// A workspace with one root agent — the shape every tool window runs in.
fn agent() -> (tempfile::TempDir, PathBuf) {
    let (holder, ws) = fixture::workspace();
    fixture::spawn_root(&ws, "a");
    (holder, ws)
}

fn held(id: &str) -> Held {
    Held {
        tool_use_id: id.into(),
        tool: "bash".into(),
        reason: "needs review\nof the diff".into(),
    }
}

#[test]
fn the_mark_ref_lives_in_the_shared_per_agent_mark_namespace() {
    // §9.2's retention delete enumerates `refs/litany/`, so a mark that
    // spelled its own root would outlive the agent it belongs to.
    assert_eq!(hold_ref("a-b"), "refs/litany/held/a-b");
}

#[test]
fn an_unset_mark_reads_as_none_the_ordinary_unparked_state() {
    let (_h, ws) = agent();
    assert_eq!(read(&ws, "a", &RealGit::new()), None);
}

#[test]
fn a_written_mark_round_trips_including_a_multiline_reason() {
    // The blob is one line of JSON, so the trimmed-UTF-8 capture round
    // trip preserves a reason that itself contains newlines.
    let (_h, ws) = agent();
    let git = RealGit::new();
    write(&ws, "a", &held("toolu_1"), &git).unwrap();
    assert_eq!(read(&ws, "a", &git), Some(held("toolu_1")));
}

#[test]
fn a_second_write_wins_restating_the_frontier() {
    let (_h, ws) = agent();
    let git = RealGit::new();
    write(&ws, "a", &held("toolu_1"), &git).unwrap();
    write(&ws, "a", &held("toolu_2"), &git).unwrap();
    assert_eq!(read(&ws, "a", &git).unwrap().tool_use_id, "toolu_2");
}

#[test]
fn clear_lifts_the_mark() {
    let (_h, ws) = agent();
    let git = RealGit::new();
    write(&ws, "a", &held("toolu_1"), &git).unwrap();
    clear(&ws, "a", &git).unwrap();
    assert_eq!(read(&ws, "a", &git), None);
}

#[test]
fn a_corrupt_blob_reads_as_none_falling_back_to_the_loud_decline() {
    let (_h, ws) = agent();
    let git = RealGit::new();
    let repo = repo_git(&ws);
    let staged = repo.join("garbage.tmp");
    std::fs::write(&staged, "not json").unwrap();
    let sha = git
        .run_capture(
            &repo,
            &["hash-object", "-w", "--", &staged.to_string_lossy()],
        )
        .unwrap();
    git.run(&repo, &["update-ref", &hold_ref("a"), &sha])
        .unwrap();
    assert_eq!(read(&ws, "a", &git), None);
}

#[test]
fn hashing_the_value_leaves_nothing_behind_beside_the_repo() {
    let (_h, ws) = agent();
    write(&ws, "a", &held("toolu_1"), &RealGit::new()).unwrap();
    let leftovers: Vec<_> = std::fs::read_dir(repo_git(&ws))
        .unwrap()
        .filter_map(|e| e.ok())
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| n.starts_with("hold-mark."))
        .collect();
    assert!(leftovers.is_empty(), "staged temp survived: {leftovers:?}");
}
