//! The extract at the landing (`docs/DESIGN_CONTEXT_ECONOMY.md` §5.3),
//! against a real git repo: the base carries `summary/NNN.refs.md`
//! beside the summary, an omitted `extract_bytes` writes none, and a
//! deletion with no summary has nothing to sit beside. The pure
//! rendering arms live in [`super::super::extract::tests`], the
//! pair-shedding arm in the assembler's.

use super::*;

/// A model-output entry naming a path, a sha and a pull request.
const SAID: &str = r#"[{"type":"text","text":"landed #12 as ab12cde in src/prompt/land.rs"}]"#;

/// Land a compaction that deletes the two seeded transcript entries and
/// writes `summary/001.md`, under `extract_bytes`, returning the
/// worktree's directory handle.
fn landed(extract_bytes: Option<usize>, summary: &[(&str, &str)]) -> TempDir {
    let dir = repo(&[
        ("messages/001-user.md", "hi\n"),
        ("messages/002-user.md", "ship the widget\n"),
        ("messages/003-m.json", SAID),
    ]);
    let wt = dir.path();
    compactor(
        wt,
        summary,
        &["messages/002-user.md", "messages/003-m.json"],
        &[],
        &[],
    );
    assert_eq!(
        land(wt, "p1", "p1-cmp", extract_bytes, &g()).unwrap(),
        LandOutcome::Landed
    );
    dir
}

#[test]
fn the_base_carries_the_extract_beside_the_summary() {
    let dir = landed(Some(4096), &[("summary/001.md", "digest\n")]);
    let wt = dir.path();
    assert!(wt.join("summary/001.md").exists(), "the summary landed");
    let refs = std::fs::read_to_string(wt.join("summary/001.refs.md")).unwrap();
    // Every section the removed span carried, and nothing from the entry
    // the compaction did not remove (`001-user.md` stays in context).
    for want in [
        "ship the widget",
        "- #12\n",
        "- ab12cde\n",
        "- src/prompt/land.rs\n",
    ] {
        assert!(refs.contains(want), "{want} missing from {refs}");
    }
    assert!(
        !refs.lines().any(|l| l.trim() == "hi"),
        "only what left context: {refs}"
    );
    // It is a compaction product like the summary: the base commit
    // carries it, so it is in the branch's tree and in its history.
    assert_eq!(
        g().run_capture(wt, &["log", "--format=%s", "--", "summary/001.refs.md"])
            .unwrap(),
        "compaction base [p1-cmp]"
    );
    assert_eq!(g().run_capture(wt, &["status", "--porcelain"]).unwrap(), "");
}

#[test]
fn an_omitted_extract_bytes_writes_no_extract() {
    // Severable (`docs/PRINCIPLES.md`): omitting the key deletes the
    // product, not a code path.
    let dir = landed(None, &[("summary/001.md", "digest\n")]);
    assert!(dir.path().join("summary/001.md").exists());
    assert!(!dir.path().join("summary/001.refs.md").exists());
}

#[test]
fn a_pass_that_wrote_no_summary_has_nothing_for_an_extract_to_sit_beside() {
    // The extract annotates a summary and sheds with it (§5.3); a
    // deletions-only pass leaves no `NNN` for it to take, so none is
    // written — the general path with empty inputs.
    let dir = landed(Some(4096), &[]);
    let wt = dir.path();
    assert!(!wt.join("messages/002-user.md").exists(), "deletion landed");
    assert!(
        std::fs::read_dir(wt.join("summary")).is_err(),
        "no summary, no extract"
    );
}

#[test]
fn a_removal_of_nothing_referable_writes_no_extract() {
    // The cap is spent on references; a span that carried none leaves
    // nothing to state, so the pair is just the summary again.
    let dir = repo(&[
        ("messages/001-user.md", "hi\n"),
        ("messages/002-m.json", "[]"),
    ]);
    let wt = dir.path();
    compactor(
        wt,
        &[("summary/001.md", "digest\n")],
        &["messages/002-m.json"],
        &[],
        &[],
    );
    assert_eq!(
        land(wt, "p1", "p1-cmp", Some(4096), &g()).unwrap(),
        LandOutcome::Landed
    );
    assert!(!wt.join("summary/001.refs.md").exists());
}
