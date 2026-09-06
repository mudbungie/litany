//! The bl-47b4 regression: the `make promote-changelog` landing is exempt
//! **by its diff**, whatever its subject says.
//!
//! Split out of the parent file to keep it under the 300-line cap; it
//! reaches [`super::is_changelog_promotion`] as a child module.

use super::is_changelog_promotion;
use std::path::Path;
use std::process::Command;

/// A throwaway repository with one commit per `(subject, changelog, other)`
/// step, returning each step's sha in order. `other` writes a second file, so
/// a step can be a delivery that also edits the changelog.
fn history(root: &Path, steps: &[(&str, &str, Option<&str>)]) -> Vec<String> {
    let sh = |args: &[&str]| {
        let out = Command::new("git")
            .current_dir(root)
            .env_remove("GIT_DIR")
            .env_remove("GIT_WORK_TREE")
            .env_remove("GIT_INDEX_FILE")
            .env("GIT_AUTHOR_NAME", "t")
            .env("GIT_AUTHOR_EMAIL", "t@t")
            .env("GIT_COMMITTER_NAME", "t")
            .env("GIT_COMMITTER_EMAIL", "t@t")
            .args(args)
            .output()
            .unwrap();
        assert!(out.status.success(), "git {args:?}: {out:?}");
        String::from_utf8_lossy(&out.stdout).trim().to_owned()
    };
    sh(&["init", "-b", "main"]);
    steps
        .iter()
        .map(|(subject, changelog, other)| {
            std::fs::write(root.join("CHANGELOG.md"), changelog).unwrap();
            if let Some(body) = other {
                std::fs::write(root.join("src.rs"), body).unwrap();
            }
            sh(&["add", "-A"]);
            sh(&["commit", "-m", subject]);
            sh(&["rev-parse", "HEAD"])
        })
        .collect()
}

/// A promotion is recognized by what it did; a delivery that also touches the
/// changelog, and a changelog edit that adds no version heading, are not.
#[test]
fn the_promotion_landing_is_read_off_its_diff_and_not_its_wording() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let base = "# Changelog\n\n## [Unreleased]\n\n- **a thing** [bl-aaaa]\n";
    let promoted = "# Changelog\n\n## [Unreleased]\n\n## [0.0.10](u/c/a...b) - 2026-09-04\n\n- **a thing** [bl-aaaa]\n";
    let shas = history(
        root,
        &[
            ("seed [bl-aaaa]", base, Some("fn a() {}\n")),
            // The very shape bl-0644 wore: the act, worded as a ball title.
            (
                "promote the accumulated [Unreleased] section to 0.0.10 [bl-0644]",
                promoted,
                None,
            ),
            (
                "a bullet was missing [bl-bbbb]",
                &format!("{promoted}- **another** [bl-bbbb]\n"),
                None,
            ),
            (
                "a delivery that also promotes [bl-cccc]",
                promoted,
                Some("fn b() {}\n"),
            ),
        ],
    );
    assert!(
        !is_changelog_promotion(root, &shas[0]),
        "the seed is not one"
    );
    assert!(
        is_changelog_promotion(root, &shas[1]),
        "the promotion landing is exempt with no phrase in its subject"
    );
    assert!(
        !is_changelog_promotion(root, &shas[2]),
        "a changelog-only edit that adds no version heading is a delivery"
    );
    assert!(
        !is_changelog_promotion(root, &shas[3]),
        "a commit that also changes source is a delivery, whatever it did to the changelog"
    );
}
