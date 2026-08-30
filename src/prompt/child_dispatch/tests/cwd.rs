//! The seeded working directory at the dispatch fork (ARCH §3.3,
//! `litany dispatch --cwd`): the child's own mark is written before the
//! fork, and nothing is inherited from the dispatcher. Split from
//! `tests.rs` for the 300-line repo cap.

use super::*;
use crate::workspace::cwd;

#[test]
fn a_seeded_child_starts_in_the_directory_its_dispatcher_named() {
    let (_h, ws) = fixture::workspace();
    let parent_wt = fixture::spawn_root(&ws, "20260101-p1");
    let g = crate::template::RealGit::new();
    let outside = tempfile::TempDir::new().unwrap();
    let target = std::fs::canonicalize(outside.path()).unwrap();

    let mut request = req(&ws, "20260101-p1", &parent_wt, "g");
    request.cwd = Some(&target);
    let child = run(
        &request,
        &g,
        &crate::prompt::SystemClock,
        &crate::prompt::NanoIdGen,
        &RecordingLauncher::ok(),
        &test_rng(),
    )
    .unwrap();

    // The mark the executor reads at every tool spawn is already set
    // when the child's first step runs.
    assert_eq!(cwd::read(&ws, &child, &g), Some(target));
}

#[test]
fn an_unseeded_child_has_no_mark_so_it_works_in_its_worktree() {
    let (_h, ws) = fixture::workspace();
    let parent_wt = fixture::spawn_root(&ws, "20260101-p1");
    let g = crate::template::RealGit::new();
    let child = run(
        &req(&ws, "20260101-p1", &parent_wt, "g"),
        &g,
        &crate::prompt::SystemClock,
        &crate::prompt::NanoIdGen,
        &RecordingLauncher::ok(),
        &test_rng(),
    )
    .unwrap();
    assert_eq!(cwd::read(&ws, &child, &g), None);
}

#[test]
fn a_dispatchers_own_working_directory_is_never_inherited() {
    // yog's writer-isolation law (yog VISION §4.10 — no two
    // write-capable lineages share a mutable checkout) rests on absence
    // being the default: a mark is keyed by agent id, and no fork,
    // merge or transfer moves one.
    let (_h, ws) = fixture::workspace();
    let parent_wt = fixture::spawn_root(&ws, "20260101-p1");
    let g = crate::template::RealGit::new();
    let elsewhere = tempfile::TempDir::new().unwrap();
    cwd::write(&ws, "20260101-p1", elsewhere.path(), &g).unwrap();

    let child = run(
        &req(&ws, "20260101-p1", &parent_wt, "g"),
        &g,
        &crate::prompt::SystemClock,
        &crate::prompt::NanoIdGen,
        &RecordingLauncher::ok(),
        &test_rng(),
    )
    .unwrap();

    assert_eq!(cwd::read(&ws, &child, &g), None);
    // The dispatcher stayed where it was: seeding writes the child's
    // mark and reads nobody's.
    assert_eq!(
        cwd::read(&ws, "20260101-p1", &g),
        Some(elsewhere.path().to_path_buf())
    );
}
