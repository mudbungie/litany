//! The snapshot step of `litany config`'s later-commit path — its
//! validation failures (bl-e3f5) and its **third source**, the
//! checkout's own workspace skills (`docs/DESIGN_LEARNING_LOOP.md` §3).
//! Split out of `tests.rs` to keep it under the 300-line cap. Mirrors
//! `template::tests_descriptions`, the `litany new` case: both verbs
//! call the same [`crate::template::descriptions::snapshot`].

use super::tests::{show, workspace, write_files};
use super::{Error, Origin, Pass, author};
use crate::template::{GitRunner, RealGit, descriptions};
use crate::workspace::{config_ref, repo_git};
use std::fs;

/// A `SKILL.md` the snapshot's parser accepts.
fn manifest(name: &str) -> String {
    format!("---\nname: {name}\ndescription: what {name} is for\n---\nbody\n")
}

#[test]
fn malformed_skill_frontmatter_declines_naming_the_file_no_commit() {
    // The same YAML plain-scalar trap as `litany new` (bl-e3f5): the
    // snapshot must decline before anything is committed, name the
    // offending pool file, and move nothing.
    let (holder, ws) = workspace();
    let data_root = holder.path().join("data");
    fs::create_dir_all(data_root.join("skills/trap")).unwrap();
    fs::write(
        data_root.join("skills/trap/SKILL.md"),
        "---\nname: trap\ndescription: posts to slack: general\n---\n",
    )
    .unwrap();
    let git = RealGit::new();
    let repo = repo_git(&ws);
    let before = git
        .run_capture(&repo, &["rev-parse", &config_ref("default")])
        .unwrap();
    let err = author(
        &ws,
        &data_root,
        "default",
        Origin::Advance,
        write_files(&[("providers.yaml", "roles: {}\n")]),
        &git,
    )
    .unwrap_err();
    match &err {
        Error::Descriptions(descriptions::Error::SkillFrontmatter { name, .. }) => {
            assert_eq!(name, "trap");
        }
        other => panic!("expected Descriptions(SkillFrontmatter), got {other:?}"),
    }
    assert!(err.to_string().contains("SKILL.md"), "{err}");
    // No `.config-author` left behind, and the branch tip is unmoved.
    assert!(!ws.join(".config-author").exists());
    let after = git
        .run_capture(&repo, &["rev-parse", &config_ref("default")])
        .unwrap();
    assert_eq!(after, before, "branch must not move on a declined pass");
}

#[test]
fn malformed_tool_schema_declines_naming_the_file_no_commit() {
    let (holder, ws) = workspace();
    let data_root = holder.path().join("data");
    fs::create_dir_all(data_root.join("tools")).unwrap();
    fs::write(data_root.join("tools/broken.json"), "{ not json").unwrap();
    let err = author(
        &ws,
        &data_root,
        "default",
        Origin::Advance,
        write_files(&[("providers.yaml", "roles: {}\n")]),
        &RealGit::new(),
    )
    .unwrap_err();
    match &err {
        Error::Descriptions(descriptions::Error::ToolSchema { name, .. }) => {
            assert_eq!(name, "broken");
        }
        other => panic!("expected Descriptions(ToolSchema), got {other:?}"),
    }
    assert!(!ws.join(".config-author").exists());
}

#[test]
fn a_workspace_skills_frontmatter_is_snapshotted_beside_the_pools() {
    // The lineage's own body at `skills/<name>/` is described exactly as
    // a pooled one is — one mechanism, a third source (§3).
    let (holder, ws) = workspace();
    let data_root = holder.path().join("data");
    let body = manifest("notes");
    let pass = author(
        &ws,
        &data_root,
        "default",
        Origin::Advance,
        write_files(&[("skills/notes/SKILL.md", body.as_str())]),
        &RealGit::new(),
    )
    .unwrap();
    assert_eq!(pass, Pass::Landed);

    let tip = config_ref("default");
    let described = show(&ws, &format!("{tip}:descriptions/skills/notes.md")).unwrap();
    assert!(
        described.contains("description: what notes is for"),
        "{described}"
    );
    // The body itself stays where it was authored — the description is
    // the snapshot, the body is what `load_skill` elects.
    assert!(show(&ws, &format!("{tip}:skills/notes/SKILL.md")).is_ok());
}

#[test]
fn a_workspace_skill_named_like_a_pool_skill_is_refused_before_any_commit() {
    // Names are unique across the two homes, which is what lets
    // `load_skill` resolve the tip first and the pool second with no
    // shadowing arm (§3).
    let (holder, ws) = workspace();
    let data_root = holder.path().join("data");
    fs::create_dir_all(data_root.join("skills/git-ops")).unwrap();
    fs::write(
        data_root.join("skills/git-ops/SKILL.md"),
        manifest("git-ops"),
    )
    .unwrap();
    let git = RealGit::new();
    let repo = repo_git(&ws);
    let before = git
        .run_capture(&repo, &["rev-parse", &config_ref("default")])
        .unwrap();

    let body = manifest("git-ops");
    let err = author(
        &ws,
        &data_root,
        "default",
        Origin::Advance,
        write_files(&[("skills/git-ops/SKILL.md", body.as_str())]),
        &git,
    )
    .unwrap_err();
    match &err {
        Error::Descriptions(descriptions::Error::PoolNameCollision { name }) => {
            assert_eq!(name, "git-ops");
        }
        other => panic!("expected Descriptions(PoolNameCollision), got {other:?}"),
    }
    assert!(!ws.join(".config-author").exists());
    let after = git
        .run_capture(&repo, &["rev-parse", &config_ref("default")])
        .unwrap();
    assert_eq!(after, before, "branch must not move on a refused pass");
}

#[test]
fn an_archived_body_is_described_nowhere() {
    // `skills/archived/<name>/` is the archive container (§5): the
    // snapshot skips it, so neither the container nor what it holds
    // reaches `descriptions/skills/`.
    let (holder, ws) = workspace();
    let data_root = holder.path().join("data");
    let notes = manifest("notes");
    let old = manifest("old");
    author(
        &ws,
        &data_root,
        "default",
        Origin::Advance,
        write_files(&[
            ("skills/notes/SKILL.md", notes.as_str()),
            ("skills/archived/old/SKILL.md", old.as_str()),
        ]),
        &RealGit::new(),
    )
    .unwrap();

    let tip = config_ref("default");
    assert!(show(&ws, &format!("{tip}:descriptions/skills/notes.md")).is_ok());
    assert!(show(&ws, &format!("{tip}:descriptions/skills/archived.md")).is_err());
    assert!(show(&ws, &format!("{tip}:descriptions/skills/old.md")).is_err());
    // The body is still committed — archival is a move, not a delete.
    assert!(show(&ws, &format!("{tip}:skills/archived/old/SKILL.md")).is_ok());
}
