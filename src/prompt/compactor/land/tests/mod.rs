//! Tests for the compaction landing (ARCH §2.6, rebase-forward).
//!
//! The behavioral arms run against a **real** git repo, so the base
//! mint, the span squash, the replay, and the four pins — live-branch-wins
//! on a work-product modify/delete, the content-conflict decline, the
//! superseded pass, and the fork-time prunes never crossing — are
//! exercised end to end. The git-op error arms live in [`stub`].

use super::*;
use crate::prompt::compactor::state;
use crate::template::{GitRunner, RealGit};
use tempfile::TempDir;

fn g() -> RealGit {
    RealGit::new()
}

/// A repo checked out on `agents/p1`, founded by the root dispatch
/// commit (`step 001: dispatch [p1]`) carrying `files`.
fn repo(files: &[(&str, &str)]) -> TempDir {
    let dir = TempDir::new().unwrap();
    let wt = dir.path();
    let git = g();
    git.run(wt, &["init", "-b", "agents/p1"]).unwrap();
    git.run(wt, &["config", "user.email", "t@t"]).unwrap();
    git.run(wt, &["config", "core.hooksPath", "/dev/null"])
        .unwrap();
    git.run(wt, &["config", "user.name", "t"]).unwrap();
    commit(wt, "step 001: dispatch [p1]", files, &[]);
    dir
}

fn write(wt: &Path, rel: &str, content: &str) {
    let path = wt.join(rel);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, content).unwrap();
}

/// One commit on the current branch: `writes` written, `deletes` removed.
fn commit(wt: &Path, subject: &str, writes: &[(&str, &str)], deletes: &[&str]) {
    let git = g();
    for (rel, content) in writes {
        write(wt, rel, content);
    }
    for rel in deletes {
        git.run(wt, &["rm", "-q", "--", rel]).unwrap();
    }
    git.run(wt, &["add", "-A"]).unwrap();
    git.run(wt, &["commit", "--allow-empty", "-m", subject])
        .unwrap();
}

/// Fork `agents/p1-cmp` off the current tip — the compaction point — with
/// a real dispatch commit (rewriting `goal.md`, pruning `prune` paths as
/// the harness does at fork, §3.3), then a compaction commit landing
/// `summary` + `dialog` writes and the nominated `deletions`. Ends back
/// on `agents/p1`.
fn compactor(
    wt: &Path,
    summary: &[(&str, &str)],
    deletions: &[&str],
    dialog: &[(&str, &str)],
    prune: &[&str],
) {
    let git = g();
    git.run(wt, &["checkout", "-q", "-b", "agents/p1-cmp"])
        .unwrap();
    commit(
        wt,
        "dispatch: compactor [p1-cmp]",
        &[("goal.md", "compact the branch\n")],
        prune,
    );
    let writes: Vec<(&str, &str)> = summary.iter().chain(dialog).copied().collect();
    commit(wt, "compaction", &writes, deletions);
    git.run(wt, &["checkout", "-q", "agents/p1"]).unwrap();
}

fn head(wt: &Path) -> String {
    g().run_capture(wt, &["rev-parse", "HEAD"]).unwrap()
}

fn subjects(wt: &Path) -> String {
    g().run_capture(wt, &["log", "--format=%s"]).unwrap()
}

#[test]
fn a_landing_squashes_the_span_and_replays_the_live_tail() {
    // §2.6 rebase-forward end to end: the span (two steps past the
    // founding commit) squashes into one compaction base carrying the
    // summary and the deletion; the live commits made while the compactor
    // ran replay on top, verbatim; the branch stays attached and clean.
    let dir = repo(&[("messages/001-user.md", "hi\n")]);
    let wt = dir.path();
    commit(wt, "step 002", &[("messages/002-a.md", "a\n")], &[]);
    commit(wt, "step 003", &[("messages/003-b.md", "b\n")], &[]);
    compactor(
        wt,
        &[("summary/001.md", "digest\n")],
        &["messages/001-user.md"],
        &[],
        &[],
    );
    commit(wt, "step 004", &[("messages/004-c.md", "c\n")], &[]);

    assert_eq!(
        land(wt, "p1", "p1-cmp", None, &g()).unwrap(),
        LandOutcome::Landed
    );
    // The product landed and the live tail survived.
    assert!(!wt.join("messages/001-user.md").exists(), "deletion landed");
    assert!(wt.join("summary/001.md").exists(), "summary landed");
    assert!(wt.join("messages/004-c.md").exists(), "live append kept");
    // History: founding commit, base, replayed tail — the span's step
    // commits are squashed out; nothing is a merge.
    let log = subjects(wt);
    assert_eq!(
        log.lines().collect::<Vec<_>>(),
        vec![
            "step 004",
            "compaction base [p1-cmp]",
            "step 001: dispatch [p1]"
        ],
        "{log}"
    );
    assert!(
        g().run_capture(wt, &["rev-list", "--merges", "HEAD"])
            .unwrap()
            .trim()
            .is_empty(),
        "nothing merges anywhere (§2.6)"
    );
    // The branch is attached and the worktree clean.
    assert_eq!(
        g().run_capture(wt, &["symbolic-ref", "--short", "HEAD"])
            .unwrap(),
        "agents/p1"
    );
    assert_eq!(g().run_capture(wt, &["status", "--porcelain"]).unwrap(), "");
    // The checkpoint clock now measures from the base: one commit since.
    let s = state(wt, "p1", 0, false, &g()).unwrap();
    assert_eq!(s.commits_since_checkpoint, 1, "the replayed tail counts");
    // The squashed span stays recoverable from the compactor's own ref:
    // its dispatch commit still reaches the pre-squash history.
    assert!(
        g().run_capture(
            wt,
            &["cat-file", "-e", "agents/p1-cmp~1:messages/001-user.md"]
        )
        .is_ok()
    );
}

#[test]
fn live_branch_wins_on_a_work_product_the_compaction_deleted() {
    // THE PIN (§2.6): a replayed commit rewrote a work product the
    // compactor nominated. The replay keeps the live content — the
    // deletion is dropped. Lost compaction, never lost work.
    let dir = repo(&[("code.txt", "v1\n")]);
    let wt = dir.path();
    compactor(
        wt,
        &[("summary/001.md", "digest\n")],
        &["code.txt"],
        &[],
        &[],
    );
    commit(wt, "step 002 rewrite", &[("code.txt", "v2\n")], &[]);

    assert_eq!(
        land(wt, "p1", "p1-cmp", None, &g()).unwrap(),
        LandOutcome::Landed
    );
    assert_eq!(
        std::fs::read_to_string(wt.join("code.txt")).unwrap(),
        "v2\n",
        "live-branch-wins: the rewritten work product survives"
    );
    assert!(wt.join("summary/001.md").exists(), "summary still landed");
    assert_eq!(g().run_capture(wt, &["status", "--porcelain"]).unwrap(), "");
}

/// Assert `err` is the git failure of operation `want`.
fn assert_op(err: Error, want: &str) {
    match err {
        Error::Git { op, .. } => assert_eq!(op, want),
        other => panic!("{other:?}"),
    }
}

mod edges;
mod extract;
mod stub;
