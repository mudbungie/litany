//! The model mark against a real workspace (ARCH §4.3 *The model mark*).

use super::{ModelMark, clear, model_ref, read, write};
use crate::config::Effort;
use crate::template::{GitRunner, RealGit};
use crate::workspace::{fixture, repo_git};
use std::path::PathBuf;

/// A workspace with one root agent — the shape a pick addresses.
fn agent() -> (tempfile::TempDir, PathBuf) {
    let (holder, ws) = fixture::workspace();
    fixture::spawn_root(&ws, "a");
    (holder, ws)
}

fn pick() -> ModelMark {
    ModelMark {
        provider: "anthropic".into(),
        model: "claude-opus-4-1".into(),
        effort: Some(Effort::High),
        priority: Some(true),
    }
}

#[test]
fn the_mark_ref_lives_in_the_shared_per_agent_mark_namespace() {
    // §9.2's retention delete enumerates `refs/litany/`, so a mark that
    // spelled its own root would outlive the agent it belongs to.
    assert_eq!(model_ref("a-b"), "refs/litany/model/a-b");
}

#[test]
fn an_unset_mark_reads_as_none_which_is_every_agents_ordinary_state() {
    let (_h, ws) = agent();
    assert_eq!(read(&ws, "a", &RealGit::new()).unwrap(), None);
}

#[test]
fn a_written_mark_reads_back_the_whole_binding() {
    let (_h, ws) = agent();
    let git = RealGit::new();
    write(&ws, "a", &pick(), &git).unwrap();
    assert_eq!(read(&ws, "a", &git).unwrap(), Some(pick()));
}

#[test]
fn the_knobs_are_optional_and_absent_round_trips_as_absent() {
    let (_h, ws) = agent();
    let git = RealGit::new();
    let bare = ModelMark {
        provider: "openai".into(),
        model: "gpt-5".into(),
        effort: None,
        priority: None,
    };
    write(&ws, "a", &bare, &git).unwrap();
    assert_eq!(read(&ws, "a", &git).unwrap(), Some(bare.clone()));
    // The bytes are the `roles:` entry's spelling for the two fields —
    // nothing written for a knob nobody asked for.
    assert_eq!(bare.to_yaml(), "provider: openai\nmodel: gpt-5\n");
}

#[test]
fn a_second_mark_wins_so_picking_again_is_just_marking_again() {
    let (_h, ws) = agent();
    let git = RealGit::new();
    write(&ws, "a", &pick(), &git).unwrap();
    let second = ModelMark {
        model: "claude-haiku-4-5".into(),
        ..pick()
    };
    write(&ws, "a", &second, &git).unwrap();
    assert_eq!(read(&ws, "a", &git).unwrap(), Some(second));
}

#[test]
fn the_mark_is_per_agent_and_does_not_descend() {
    // No inheritance by descent (§4.3): a pick on the root is not a pick
    // on its child, whose role has its own `roles:` entry.
    let (_h, ws) = agent();
    let git = RealGit::new();
    fixture::spawn_agent(&ws, "a-b", &crate::workspace::agent_ref("a"));
    write(&ws, "a", &pick(), &git).unwrap();
    assert_eq!(read(&ws, "a-b", &git).unwrap(), None);
}

#[test]
fn clearing_returns_the_agent_to_its_roles_own_binding() {
    let (_h, ws) = agent();
    let git = RealGit::new();
    write(&ws, "a", &pick(), &git).unwrap();
    clear(&ws, "a", &git).unwrap();
    assert_eq!(read(&ws, "a", &git).unwrap(), None);
}

#[test]
fn hashing_the_value_leaves_nothing_behind_beside_the_repo() {
    let (_h, ws) = agent();
    write(&ws, "a", &pick(), &RealGit::new()).unwrap();
    let leftovers: Vec<_> = std::fs::read_dir(repo_git(&ws))
        .unwrap()
        .flatten()
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| n.starts_with("model-mark."))
        .collect();
    assert!(
        leftovers.is_empty(),
        "staged value left behind: {leftovers:?}"
    );
}

#[test]
fn a_mark_pointing_at_a_non_blob_reads_as_none() {
    // A ref at a commit is not a mark anyone wrote through this module;
    // the role's own binding applies rather than a step failing on it.
    let (_h, ws) = agent();
    let git = RealGit::new();
    let repo = repo_git(&ws);
    let head = git
        .run_capture(&repo, &["rev-parse", "refs/heads/config/default"])
        .unwrap();
    git.run(&repo, &["update-ref", &model_ref("a"), head.trim()])
        .unwrap();
    assert_eq!(read(&ws, "a", &git).unwrap(), None);
}

#[test]
fn bytes_that_do_not_parse_are_an_error_not_a_silent_fallback() {
    // A hand-written ref the operator believes stands must not resolve
    // the lineage's binding underneath them (§4.3).
    let (_h, ws) = agent();
    let git = RealGit::new();
    let repo = repo_git(&ws);
    let staged = repo.join("junk.tmp");
    std::fs::write(&staged, "model: only-half\n").unwrap();
    let blob = git
        .run_capture(
            &repo,
            &["hash-object", "-w", "--", &staged.to_string_lossy()],
        )
        .unwrap();
    git.run(&repo, &["update-ref", &model_ref("a"), blob.trim()])
        .unwrap();
    let err = read(&ws, "a", &git).unwrap_err().to_string();
    assert!(err.contains("refs/litany/model/a"), "{err}");
    assert!(err.starts_with("config: parse"), "{err}");
}

#[test]
fn an_empty_blob_reads_as_none() {
    let (_h, ws) = agent();
    let git = RealGit::new();
    let repo = repo_git(&ws);
    let staged = repo.join("empty.tmp");
    std::fs::write(&staged, "").unwrap();
    let blob = git
        .run_capture(
            &repo,
            &["hash-object", "-w", "--", &staged.to_string_lossy()],
        )
        .unwrap();
    git.run(&repo, &["update-ref", &model_ref("a"), blob.trim()])
        .unwrap();
    assert_eq!(read(&ws, "a", &git).unwrap(), None);
}
