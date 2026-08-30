//! Every delivery on `main` since the last release tag must have a
//! `CHANGELOG.md` bullet — the guard that keeps the hand-maintained
//! changelog complete.
//!
//! `CHANGELOG.md` is the **only** changelog authority here: release-plz never
//! writes it (`changelog_update = false` in `release-plz.toml`, bl-7558), and
//! generation could not recover a docs-only delivery even in principle — it
//! touches no packaged file, so release-plz attributes no commit to the crate.
//! That makes the hand-kept `[Unreleased]` list load-bearing, and a
//! convention nothing enforces is one that drifts: bl-0b1f back-filled 24
//! missing bullets on 2026-08-03, and bl-d92b found 8 more in the very next
//! release window. This test is the answer to the recurrence.
//!
//! **What it asserts.** Every `[bl-xxxx]` id in the subject of a commit in
//! `<last v-tag>..refs/heads/main` appears somewhere in `CHANGELOG.md`. Three
//! exclusions, all of them the changelog header's own *process, not product*:
//!
//! - **gate closes** (`gate: tests [bl-xxxx]` and the older `<kind> gate: …`
//!   spelling) are process rather than product and are deliberately not
//!   listed — they live in git and in the balls store;
//! - **release-prep commits** (`<version> release prep: …`), the
//!   `make promote-changelog` landing — process by the same rule: they carry
//!   a ball id but list no delivery;
//! - **commits with no `[bl-xxxx]` subject at all** — merge commits and
//!   release-plz's own version bump — which are not deliveries.
//!
//! It reads the whole file rather than only the `[Unreleased]` section, so it
//! keeps holding across `make promote-changelog`: the ids move under the new
//! version heading, and the last tag does not exist until the release lands.
//!
//! **When it fires.** Only after a delivery has reached `main`, so it never
//! blocks the work that is missing its bullet — it blocks the *next* close,
//! naming the ids. That is the intended pressure: the bullet is a repo
//! invariant, not one agent's chore.
//!
//! The git env is scrubbed for the reason `tests/commit_hygiene.rs` gives: a
//! run under the pre-commit gate inherits `GIT_DIR`/`GIT_INDEX_FILE` from the
//! hook that spawned it, and this must read the repo's own ref.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::process::Command;

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

/// Run a `git` query in `root` with the ambient git env scrubbed, or `None`
/// when git or the ref is unavailable — a checkout that cannot answer is not
/// a checkout this guard may judge.
fn git(root: &Path, args: &[&str]) -> Option<String> {
    let out = Command::new("git")
        .current_dir(root)
        .env_remove("GIT_DIR")
        .env_remove("GIT_WORK_TREE")
        .env_remove("GIT_INDEX_FILE")
        .args(args)
        .output()
        .ok()?;
    out.status
        .success()
        .then(|| String::from_utf8_lossy(&out.stdout).into_owned())
}

/// The `[bl-xxxx]` id a delivery subject ends with, or `None` for a subject
/// that carries none (a merge, a release bump) or that is **process rather
/// than product** — the wording `CHANGELOG.md`'s own header uses, and the one
/// rule behind both exemptions:
///
/// - a **gate close**, the `tests`/`docs`/`alignment` subtask every ball
///   carries, which lives in git and in the balls store;
/// - a **release prep** commit, the `make promote-changelog` landing that
///   stamps `[Unreleased]` as the new version. It carries a ball id but lists
///   no delivery, and a bullet saying "the changelog was promoted" is noise in
///   the very release notes it is promoting. `0ff056a` — *"0.0.6 release prep:
///   promote the changelog [Unreleased] section to [0.0.6] [bl-7cff]"* — is
///   the shape, and it is already on `main` with no bullet.
fn delivery_id(subject: &str) -> Option<String> {
    let subject = subject.trim();
    let id = subject
        .rsplit_once("[bl-")?
        .1
        .strip_suffix(']')
        .map(|rest| format!("bl-{rest}"))?;
    let process = subject.starts_with("gate:")
        || subject.contains(" gate: ")
        || subject.contains("release prep:");
    (!process).then_some(id)
}

#[test]
fn every_delivery_since_the_last_release_has_a_changelog_bullet() {
    let root = repo_root();
    let Some(tag) = last_release_tag(&root) else {
        eprintln!("changelog guard skipped: no v* tag reachable from refs/heads/main");
        return;
    };
    let range = format!("{tag}..refs/heads/main");
    let Some(log) = git(&root, &["log", "--format=%s", &range]) else {
        eprintln!("changelog guard skipped: no readable refs/heads/main under {root:?}");
        return;
    };
    let changelog = std::fs::read_to_string(root.join("CHANGELOG.md"))
        .expect("CHANGELOG.md is the changelog authority and must be readable");

    let missing: BTreeSet<String> = log
        .lines()
        .filter_map(delivery_id)
        .filter(|id| !changelog.contains(id.as_str()))
        .collect();
    assert!(
        missing.is_empty(),
        "CHANGELOG.md has no bullet for {} deliveries in {range}: {}\n\
         Every delivery adds one bullet under `## [Unreleased]` (CHANGELOG.md's header \
         states the format). Only process commits are exempt — gate closes and the \
         release-prep landing.",
        missing.len(),
        missing.into_iter().collect::<Vec<_>>().join(", "),
    );
}

/// The last release reachable from `main`: every `v*` or `litany-v*` tag
/// (the two tag eras either side of the bl-2f58 rename fence) that is an
/// ancestor, keeping the one nearest by `rev-list --count` — the same
/// counting the guard's own range uses. Not `git describe --abbrev=0`,
/// which walks candidates in committer-date order and answers wrongly
/// under date skew: this history carries a parent whose committer date
/// postdates its child by hours, and describe answered `v0.0.1` (113
/// commits out) while `v0.0.8` sat 3 commits away — widening the range
/// three releases back and failing the guard on a delivery nobody in
/// the current window touched (bl-d11e).
fn last_release_tag(root: &Path) -> Option<String> {
    let tags = git(root, &["tag", "--list", "v*", "litany-v*"])?;
    tags.split_whitespace()
        .filter(|tag| {
            git(
                root,
                &["merge-base", "--is-ancestor", tag, "refs/heads/main"],
            )
            .is_some()
        })
        .filter_map(|tag| {
            let range = format!("{tag}..refs/heads/main");
            let count = git(root, &["rev-list", "--count", &range])?;
            Some((count.trim().parse::<u64>().ok()?, tag.to_string()))
        })
        .min()
        .map(|(_, tag)| tag)
}

/// The bl-d11e regression: a history whose committer dates run backwards
/// — the old release commit stamped *later* than the new release's —
/// must still resolve the nearest tag. `git describe --abbrev=0`, the
/// retired resolution, walks by committer date and picks the old tag on
/// exactly this shape.
#[test]
fn tag_resolution_survives_committer_date_skew() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let sh = |args: &[&str], date: &str| {
        let out = Command::new("git")
            .current_dir(root)
            .env_remove("GIT_DIR")
            .env_remove("GIT_WORK_TREE")
            .env_remove("GIT_INDEX_FILE")
            .env("GIT_AUTHOR_DATE", date)
            .env("GIT_COMMITTER_DATE", date)
            .env("GIT_AUTHOR_NAME", "t")
            .env("GIT_AUTHOR_EMAIL", "t@t")
            .env("GIT_COMMITTER_NAME", "t")
            .env("GIT_COMMITTER_EMAIL", "t@t")
            .args(args)
            .output()
            .unwrap();
        assert!(out.status.success(), "git {args:?}: {out:?}");
    };
    let commit = |msg: &str, date: &str| {
        sh(&["commit", "--allow-empty", "-m", msg], date);
    };
    sh(&["init", "-b", "main"], "2026-01-01T00:00:00Z");
    // The old release, stamped FAR IN THE FUTURE — the skew.
    commit("root", "2026-06-01T00:00:00Z");
    sh(
        &["tag", "-a", "v0.0.1", "-m", "v0.0.1"],
        "2026-06-01T00:00:00Z",
    );
    // A hundred ordinary commits, then the new release with an EARLIER
    // committer date than the tag behind it.
    for i in 0..100 {
        commit(&format!("work {i}"), "2026-01-02T00:00:00Z");
    }
    commit("release v0.0.2", "2026-01-03T00:00:00Z");
    sh(
        &["tag", "-a", "v0.0.2", "-m", "v0.0.2"],
        "2026-01-03T00:00:00Z",
    );
    commit("tip", "2026-01-04T00:00:00Z");

    assert_eq!(
        last_release_tag(root).as_deref(),
        Some("v0.0.2"),
        "the nearest reachable release, regardless of committer-date order"
    );
}

/// The subject classifier, on the shapes this repo's history actually
/// carries — pinned here because everything above turns on it.
#[test]
fn delivery_ids_come_from_subjects_that_are_deliveries() {
    assert_eq!(
        delivery_id("multi_tool: let the envelope assert parallel execution [bl-ec74]").as_deref(),
        Some("bl-ec74"),
    );
    // Process, not product (CHANGELOG.md's header): gate closes in both
    // spellings, and the release-prep landing.
    assert_eq!(delivery_id("gate: alignment [bl-b03d]"), None);
    assert_eq!(delivery_id("docs gate: bl-19d5 README fix [bl-c6ee]"), None);
    assert_eq!(
        delivery_id(
            "0.0.6 release prep: promote the changelog [Unreleased] section to [0.0.6] [bl-7cff]"
        ),
        None,
    );
    // No id: a merge commit, release-plz's bump, an unrelated subject.
    assert_eq!(delivery_id("Merge pull request #6 from mudbungie/rp"), None);
    assert_eq!(delivery_id("chore: release v0.0.6"), None);
    // A malformed tail is not an id either — the guard names nothing it
    // cannot find in the log.
    assert_eq!(delivery_id("something [bl-abcd"), None);
}
