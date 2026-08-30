//! Snapshot-time validation failures (bl-e3f5), through `litany config`'s
//! later-commit path — split out of `tests.rs` to keep it under the
//! 300-line cap. Mirrors `template::tests_descriptions`, the `litany new`
//! case: both verbs call the same [`crate::template::descriptions::snapshot`].

use super::tests::{workspace, write_files};
use super::{Error, Origin, author};
use crate::template::{GitRunner, RealGit, descriptions};
use crate::workspace::{config_ref, repo_git};
use std::fs;

#[test]
fn malformed_skill_frontmatter_declines_naming_the_file_no_commit() {
    // The same YAML plain-scalar trap as `litany new` (bl-e3f5): `author`
    // must decline before `edit` even runs, name the offending pool file,
    // and move nothing.
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
