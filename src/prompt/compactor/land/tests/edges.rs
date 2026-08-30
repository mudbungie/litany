//! The decline, superseded, fallback, and error edges of the landing —
//! split from [`super`] (the happy-path behavioral arms and the shared
//! real-git helpers) to hold the per-file line cap.

use super::*;

#[test]
fn a_content_conflict_is_declined_and_marked_never_landed() {
    // THE PIN (§2.6 decline): the live branch authored the same
    // `summary/001.md` the compactor wrote — both sides carry content, so
    // git would write markers. The landing aborts: HEAD stands, the live
    // file is marker-free, and `refs/litany/conflicted/p1-cmp` names the
    // compactor's work.
    let dir = repo(&[("messages/001-user.md", "hi\n")]);
    let wt = dir.path();
    compactor(wt, &[("summary/001.md", "compactor B\n")], &[], &[], &[]);
    commit(wt, "live", &[("summary/001.md", "compactor A\n")], &[]);
    let before = head(wt);

    assert_eq!(
        land(wt, "p1", "p1-cmp", &g()).unwrap(),
        LandOutcome::Conflicted(vec!["summary/001.md".to_string()])
    );
    assert_eq!(head(wt), before, "nothing landed");
    let live = std::fs::read_to_string(wt.join("summary/001.md")).unwrap();
    assert_eq!(live, "compactor A\n");
    assert!(!live.contains("<<<<<<<"), "no conflict markers: {live}");
    assert_eq!(
        g().run_capture(wt, &["rev-parse", "refs/litany/conflicted/p1-cmp"])
            .unwrap(),
        g().run_capture(wt, &["rev-parse", "agents/p1-cmp"])
            .unwrap()
    );
}

#[test]
fn the_compactors_dialog_and_fork_prunes_never_cross() {
    // §2.6 filtered by construction: the compactor's dispatch commit
    // rewrote `goal.md` and pruned `descriptions/**` to its empty grant
    // (§3.3), and its step loop appended its own transcript. None of that
    // is product: the parent keeps its goal, its descriptors — the
    // bl-475a polarity, which the retired merge-back imported — and gains
    // zero compactor entries.
    let dir = repo(&[
        ("goal.md", "parent goal\n"),
        ("descriptions/tools/bash.json", "{}\n"),
        ("messages/001-user.md", "hi\n"),
    ]);
    let wt = dir.path();
    compactor(
        wt,
        &[("summary/001.md", "digest\n")],
        &["messages/001-user.md"],
        &[("messages/002-goal.md", "compact\n")],
        &["descriptions/tools/bash.json"],
    );

    assert_eq!(land(wt, "p1", "p1-cmp", &g()).unwrap(), LandOutcome::Landed);
    let read = |rel: &str| std::fs::read_to_string(wt.join(rel)).unwrap();
    assert_eq!(read("goal.md"), "parent goal\n", "parent keeps its goal");
    assert!(
        wt.join("descriptions/tools/bash.json").exists(),
        "the fork-time descriptions prune never reaches the parent"
    );
    assert!(!wt.join("messages/002-goal.md").exists(), "no dialog");
    assert!(!wt.join("messages/001-user.md").exists(), "deletion landed");
    assert!(wt.join("summary/001.md").exists());
}

#[test]
fn an_empty_product_is_a_noop() {
    // A final-response compactor that nominated nothing and wrote no
    // summary: nothing to land, no base minted, HEAD stands.
    let dir = repo(&[("goal.md", "g\n")]);
    let wt = dir.path();
    compactor(wt, &[], &[], &[("messages/002-goal.md", "dialog\n")], &[]);
    let before = head(wt);
    assert_eq!(land(wt, "p1", "p1-cmp", &g()).unwrap(), LandOutcome::NoOp);
    assert_eq!(head(wt), before);
}

#[test]
fn a_pass_overtaken_by_a_landing_is_superseded() {
    // A compaction landed since this compactor forked: its point was
    // rebased out of the branch's history. The pass lands nothing and
    // marks nothing — the next checkpoint trigger fires afresh.
    let dir = repo(&[("messages/001-user.md", "hi\n")]);
    let wt = dir.path();
    commit(wt, "step 002", &[("messages/002-a.md", "a\n")], &[]);
    compactor(
        wt,
        &[("summary/001.md", "digest\n")],
        &["messages/001-user.md"],
        &[],
        &[],
    );
    assert_eq!(land(wt, "p1", "p1-cmp", &g()).unwrap(), LandOutcome::Landed);
    let before = head(wt);
    // The same return interpreted again — its point is gone.
    assert_eq!(
        land(wt, "p1", "p1-cmp", &g()).unwrap(),
        LandOutcome::Superseded
    );
    assert_eq!(head(wt), before);
    assert!(
        g().run_capture(wt, &["rev-parse", "refs/litany/conflicted/p1-cmp"])
            .is_err(),
        "an overtaken pass is not a defect — nothing is marked"
    );
}

#[test]
fn a_landing_inside_the_replay_span_supersedes_the_pass() {
    // The reachable variant: the point survives, but a compaction base
    // sits in `point..HEAD`. Replaying a squash is not a landing this
    // pass can have.
    let dir = repo(&[("messages/001-user.md", "hi\n")]);
    let wt = dir.path();
    compactor(wt, &[("summary/001.md", "s\n")], &[], &[], &[]);
    commit(
        wt,
        "compaction base [p1-other]",
        &[("summary/002.md", "o\n")],
        &[],
    );
    let before = head(wt);
    assert_eq!(
        land(wt, "p1", "p1-cmp", &g()).unwrap(),
        LandOutcome::Superseded
    );
    assert_eq!(head(wt), before);
}

#[test]
fn a_branch_without_a_founding_commit_bounds_the_span_at_the_root() {
    // The general-path fallback (§2.6): no `[p1]`-founding commit and no
    // prior landing reachable from the point — the base parents on the
    // root commit, and everything between it and the point squashes.
    let dir = TempDir::new().unwrap();
    let wt = dir.path();
    let git = g();
    git.run(wt, &["init", "-b", "agents/p1"]).unwrap();
    git.run(wt, &["config", "user.email", "t@t"]).unwrap();
    git.run(wt, &["config", "core.hooksPath", "/dev/null"])
        .unwrap();
    git.run(wt, &["config", "user.name", "t"]).unwrap();
    commit(wt, "root", &[("messages/001-user.md", "hi\n")], &[]);
    commit(wt, "step", &[("messages/002-a.md", "a\n")], &[]);
    compactor(
        wt,
        &[("summary/001.md", "digest\n")],
        &["messages/001-user.md"],
        &[],
        &[],
    );
    assert_eq!(land(wt, "p1", "p1-cmp", &g()).unwrap(), LandOutcome::Landed);
    let log = subjects(wt);
    assert_eq!(
        log.lines().collect::<Vec<_>>(),
        vec!["compaction base [p1-cmp]", "root"],
        "{log}"
    );
}

#[test]
fn a_compactor_branch_without_a_dispatch_commit_is_declined_loudly() {
    let dir = repo(&[("goal.md", "g\n")]);
    let wt = dir.path();
    g().run(wt, &["branch", "agents/p1-cmp"]).unwrap();
    let err = land(wt, "p1", "p1-cmp", &g()).unwrap_err();
    assert_op(err, "compaction land dispatch commit");
}

#[test]
fn a_bad_compactor_ref_is_declined_loudly() {
    let dir = repo(&[("goal.md", "g\n")]);
    let err = land(dir.path(), "p1", "does-not-exist", &g()).unwrap_err();
    assert_op(err, "founding sha log");
}
