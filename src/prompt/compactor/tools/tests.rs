//! Unit coverage for the compactor toolset ([`super`]): summary
//! numbering, the deletion-only `git rm`, and the not-compaction-eligible
//! decline that keeps the goal and the system slot's files outside the
//! compaction-eligible set (ARCH §2.7).

use super::*;
use crate::template::RealGit;

fn tmpdir() -> tempfile::TempDir {
    tempfile::TempDir::new().unwrap()
}

#[test]
fn write_summary_picks_001_when_dir_is_empty() {
    let wt = tmpdir();
    let rel = write_summary(wt.path(), "body\n").unwrap();
    assert_eq!(rel, "summary/001.md");
    assert_eq!(
        std::fs::read_to_string(wt.path().join(&rel)).unwrap(),
        "body\n"
    );
}

#[test]
fn write_summary_increments_past_existing_files() {
    let wt = tmpdir();
    let dir = wt.path().join("summary");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("001.md"), "old").unwrap();
    std::fs::write(dir.join("007.md"), "also old").unwrap();
    let rel = write_summary(wt.path(), "new\n").unwrap();
    assert_eq!(rel, "summary/008.md");
}

#[test]
fn write_summary_skips_non_md_and_unparseable_stems() {
    let wt = tmpdir();
    let dir = wt.path().join("summary");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("README.txt"), "").unwrap();
    std::fs::write(dir.join("notes.md"), "").unwrap();
    std::fs::write(dir.join("002.md"), "").unwrap();
    let rel = write_summary(wt.path(), "x").unwrap();
    assert_eq!(rel, "summary/003.md");
}

#[test]
fn write_summary_skips_a_non_utf8_file_name() {
    // A stem `to_str` cannot decode is skipped like any other
    // operator-dropped stray, never a numbering fault.
    use std::os::unix::ffi::OsStrExt;
    let wt = tmpdir();
    let dir = wt.path().join("summary");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join(std::ffi::OsStr::from_bytes(b"\xFF\xFE")), "").unwrap();
    std::fs::write(dir.join("004.md"), "").unwrap();
    let rel = write_summary(wt.path(), "x").unwrap();
    assert_eq!(rel, "summary/005.md");
}

/// A real repo on `agents/p1` with one tracked file, for the
/// deletion-only `git rm` path.
fn repo_with(rel: &str) -> tempfile::TempDir {
    let dir = tmpdir();
    let wt = dir.path();
    let g = RealGit::new();
    g.run(wt, &["init", "-b", "agents/p1"]).unwrap();
    g.run(wt, &["config", "user.email", "t@t"]).unwrap();
    g.run(wt, &["config", "core.hooksPath", "/dev/null"])
        .unwrap();
    g.run(wt, &["config", "user.name", "t"]).unwrap();
    let f = wt.join(rel);
    std::fs::create_dir_all(f.parent().unwrap()).unwrap();
    std::fs::write(&f, "content\n").unwrap();
    g.run(wt, &["add", "-A"]).unwrap();
    g.run(wt, &["commit", "-m", "c"]).unwrap();
    dir
}

#[test]
fn mark_for_deletion_stages_a_real_removal() {
    let dir = repo_with("messages/004-user.md");
    let wt = dir.path();
    mark_for_deletion(wt, "messages/004-user.md", &RealGit::new()).unwrap();
    // Removed from the worktree and staged for the next commit.
    assert!(!wt.join("messages/004-user.md").exists());
    let staged = RealGit::new()
        .run_capture(wt, &["diff", "--cached", "--name-status"])
        .unwrap();
    assert!(staged.starts_with('D'), "staged deletion: {staged:?}");
}

#[test]
fn mark_for_deletion_declines_the_dispatch_entry_and_removes_nothing() {
    // The goal is not compaction-eligible (§2.7): the branch's opening
    // prompt is the same text `goal.md` carries and the compactor's own
    // goal quotes, so the one entry that reads as pure duplication is
    // refused at the nomination — the file is still on disk afterwards
    // and nothing is staged.
    let dir = repo_with("messages/001-user.md");
    let wt = dir.path();
    let err = mark_for_deletion(wt, "messages/001-user.md", &RealGit::new()).unwrap_err();
    assert!(
        matches!(&err, Error::NotCompactionEligible { path, .. } if path == "messages/001-user.md"),
        "{err:?}"
    );
    // The decline names the path and says why, so the model reads it
    // verbatim off the `is_error` tool_result (§3.3).
    let text = err.to_string();
    assert!(text.contains("messages/001-user.md"), "{text}");
    assert!(text.contains("not compaction-eligible"), "{text}");
    assert!(wt.join("messages/001-user.md").exists());
    let staged = RealGit::new()
        .run_capture(wt, &["diff", "--cached", "--name-status"])
        .unwrap();
    assert!(staged.trim().is_empty(), "nothing staged: {staged:?}");
}

#[test]
fn the_dispatch_entry_is_read_off_the_name_alone() {
    // Derived from the `NNN-` prefix, like the transcript's own counter:
    // any origin token and any extension at 001, a leading `./` folded,
    // and nothing outside `messages/` or past the first entry.
    for yes in [
        "messages/001-user.md",
        "./messages/001-user.md",
        "messages/001-p1-c1.md",
        "messages/001-claude-fable-5.json",
        "messages/1-user.md",
    ] {
        let what = not_compaction_eligible(yes).unwrap_or_else(|| panic!("{yes}"));
        assert!(what.contains("dispatch entry"), "{yes}: {what}");
    }
    for no in [
        "messages/002-user.md",
        "messages/010-tool.json",
        "summary/001.md",
        "messages/notes.md",
        "messages",
        "messagesX/001-user.md",
        "goal.txt",
        "src/goal.md",
    ] {
        assert!(not_compaction_eligible(no).is_none(), "{no}");
    }
}

#[test]
fn the_system_slots_files_are_read_off_the_name_alone() {
    // §5.2's three structural wire homes, plus a leading `./`. Each is
    // written at the compactor's own dispatch commit, so nominating one
    // after that lands as a deletion the dispatching branch inherits —
    // the branch would keep stepping with no goal, no soul or no
    // identity line (ARCH §2.7, §5.2).
    for yes in ["goal.md", "soul.md", "name", "./goal.md", "./name"] {
        let what = not_compaction_eligible(yes).unwrap_or_else(|| panic!("{yes}"));
        assert!(what.contains("system slot"), "{yes}: {what}");
    }
}

#[test]
fn mark_for_deletion_declines_the_system_slots_files() {
    // The whole triple, at the tool rather than at the predicate: the
    // decline is in-band (§3.3), the file survives on disk, and nothing
    // is staged — so no later commit can carry the deletion.
    for name in crate::prompt::dispatch::step_commit::SYSTEM_SLOT_FILES {
        let dir = repo_with(name);
        let wt = dir.path();
        let err = mark_for_deletion(wt, name, &RealGit::new()).unwrap_err();
        assert!(
            matches!(&err, Error::NotCompactionEligible { path, .. } if path == name),
            "{name}: {err:?}"
        );
        let text = err.to_string();
        assert!(text.contains(name), "{text}");
        assert!(text.contains("system slot"), "{text}");
        assert!(text.contains("not compaction-eligible"), "{text}");
        assert!(wt.join(name).exists(), "{name} still on disk");
        let staged = RealGit::new()
            .run_capture(wt, &["diff", "--cached", "--name-status"])
            .unwrap();
        assert!(staged.trim().is_empty(), "nothing staged: {staged:?}");
    }
}

#[test]
fn mark_for_deletion_declines_a_nonexistent_path() {
    let dir = repo_with("keep.txt");
    let err = mark_for_deletion(dir.path(), "no/such.md", &RealGit::new()).unwrap_err();
    assert!(
        matches!(
            err,
            Error::Git {
                op: "mark_for_deletion rm",
                ..
            }
        ),
        "{err:?}"
    );
}
