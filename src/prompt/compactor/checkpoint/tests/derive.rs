//! [`state`]'s git derivation, against a real repo: where the clock's
//! [`origin`] lands, what it counts from there, and the `op` tag each
//! git step carries when it fails.

use super::*;
use crate::template::RealGit;
use tempfile::TempDir;

fn init(wt: &Path) {
    let g = RealGit::new();
    g.run(wt, &["init", "-b", "agents/p1"]).unwrap();
    g.run(wt, &["config", "user.email", "t@t"]).unwrap();
    g.run(wt, &["config", "core.hooksPath", "/dev/null"])
        .unwrap();
    g.run(wt, &["config", "user.name", "t"]).unwrap();
}

fn commit(wt: &Path, subject: &str, rel: &str, content: &str) {
    let g = RealGit::new();
    let f = wt.join(rel);
    std::fs::create_dir_all(f.parent().unwrap()).unwrap();
    std::fs::write(&f, content).unwrap();
    g.run(wt, &["add", "-A"]).unwrap();
    g.run(wt, &["commit", "-m", subject]).unwrap();
}

fn now_of(wt: &Path) -> u64 {
    RealGit::new()
        .run_capture(wt, &["log", "-n", "1", "--format=%ct", "HEAD"])
        .unwrap()
        .trim()
        .parse()
        .unwrap()
}

#[test]
fn state_counts_the_whole_branch_when_no_checkpoint_landed() {
    let dir = TempDir::new().unwrap();
    let wt = dir.path();
    init(wt);
    commit(wt, "root", "a.txt", "1");
    // Anchor `now` on the root commit's own committer time (read back
    // from git while it is HEAD), so the expected elapsed is exact.
    // (Anchoring on the *second* commit's time assumed both commits
    // land in the same wall second — two real `git commit` runs under
    // load can straddle it.)
    let root_ct = now_of(wt);
    commit(wt, "step", "b.txt", "2");
    let s = state(wt, "p1", root_ct + 7, false, &RealGit::new()).unwrap();
    assert_eq!(s.commits_since_checkpoint, 2, "root + one step");
    assert_eq!(s.seconds_since_checkpoint, 7, "measured from the root");
    assert!(!s.flush_requested);
}

#[test]
fn state_measures_from_the_last_compaction_base_or_retired_merge() {
    // The clock reads the newest landing commit: a retired-mechanism merge
    // subject still counts, and a later compaction base supersedes it.
    let dir = TempDir::new().unwrap();
    let wt = dir.path();
    init(wt);
    commit(wt, "root", "a.txt", "1");
    commit(wt, "compaction merge [p1-old]", "summary/001.md", "x");
    commit(wt, "compaction base [p1-cmp]", "summary/002.md", "y");
    // Anchor `now` on the checkpoint commit's committer time (it is HEAD
    // right here) so the expected elapsed is exact, not racing the clock.
    let cmp_ct = now_of(wt);
    commit(wt, "step after", "b.txt", "2");
    let s = state(wt, "p1", cmp_ct + 42, true, &RealGit::new()).unwrap();
    assert_eq!(s.commits_since_checkpoint, 1, "only the post-landing step");
    // Elapsed is measured from the checkpoint commit, not the root.
    assert_eq!(s.seconds_since_checkpoint, 42);
    assert!(s.flush_requested);
}

#[test]
fn state_saturates_when_now_precedes_the_checkpoint() {
    let dir = TempDir::new().unwrap();
    let wt = dir.path();
    init(wt);
    commit(wt, "root", "a.txt", "1");
    let s = state(wt, "p1", 0, false, &RealGit::new()).unwrap();
    assert_eq!(s.seconds_since_checkpoint, 0, "no negative elapsed time");
}

#[test]
fn state_surfaces_a_git_failure() {
    // A non-repo directory: the first rev-parse/log fails loudly.
    let dir = TempDir::new().unwrap();
    let err = state(dir.path(), "p1", 0, false, &RealGit::new()).unwrap_err();
    assert!(matches!(err, Error::Git { .. }), "{err:?}");
}

#[test]
fn state_measures_from_the_branchs_own_dispatch_commit_not_inherited_history() {
    // The recursion of bl-a9eb (yog bl-ebbd) in miniature: a child forks
    // off a parent that already has a long history, so counting from the
    // *root* commit would read the parent's commits as the child's and
    // trip `every_n_commits` on a seconds-old branch.
    let dir = TempDir::new().unwrap();
    let wt = dir.path();
    init(wt);
    for i in 0..25 {
        commit(
            wt,
            &format!("inherited step {i}"),
            &format!("m/{i}.md"),
            "x",
        );
    }
    commit(wt, "dispatch: compactor [p1-c1]", "goal.md", "g");
    commit(wt, "step 001", "m/own.md", "x");
    let s = state(wt, "p1-c1", now_of(wt), false, &RealGit::new()).unwrap();
    assert_eq!(
        s.commits_since_checkpoint, 1,
        "only this branch's own commit, not the 26 it inherited"
    );
    let c = cfg(CompactionTrigger::EveryNCommits, Some(20));
    assert!(
        !due(Some(&c), &s).unwrap(),
        "a seconds-old branch is below threshold"
    );
}

#[test]
fn state_measures_from_the_dispatch_commit_not_a_later_transcript_commit() {
    // Production subject shapes, which the synthetic ones above never
    // exercised: the executor's transcript commits end in the *same*
    // `[<agent-id>]` tail as the dispatch commit that founds the branch
    // (`transcript NNN: <origin> [<id>]`, `dispatch/transcript.rs`; the
    // stray recovery's `transcript: recover delivered stray [<id>]`
    // likewise). Matching that tail alone made `origin` answer with the
    // *newest* transcript commit, so the clock read ~0 forever and
    // `every_n_commits` never fired (bl-89f7).
    let dir = TempDir::new().unwrap();
    let wt = dir.path();
    init(wt);
    commit(wt, "step 001: dispatch [p1]", "goal.md", "g");
    commit(wt, "transcript 001: user [p1]", "t/001.md", "x");
    commit(wt, "transcript 002: assistant [p1]", "t/002.md", "x");
    let s = state(wt, "p1", now_of(wt), false, &RealGit::new()).unwrap();
    assert_eq!(
        s.commits_since_checkpoint, 2,
        "the two transcript commits since dispatch, not 0"
    );
    assert!(due(Some(&cfg(CompactionTrigger::EveryNCommits, Some(2))), &s).unwrap());

    // Same for a child branch, whose founding is `dispatch: <role> [<id>]`,
    // and for the stray-recovery subject.
    commit(wt, "dispatch: worker [p1-c1]", "child.md", "c");
    commit(
        wt,
        "transcript: recover delivered stray [p1-c1]",
        "s.md",
        "x",
    );
    let child = state(wt, "p1-c1", now_of(wt), false, &RealGit::new()).unwrap();
    assert_eq!(child.commits_since_checkpoint, 1);
}

#[test]
fn state_reads_the_branch_role_from_its_own_dispatch_commit() {
    let dir = TempDir::new().unwrap();
    let wt = dir.path();
    init(wt);
    commit(wt, "root", "a.txt", "1");
    commit(wt, "dispatch: compactor [p1-c1]", "goal.md", "g");
    let s = state(wt, "p1-c1", now_of(wt), false, &RealGit::new()).unwrap();
    assert!(s.is_compactor, "role derived from the dispatch subject");

    // A worker dispatch off the same shape is not a compactor, and a
    // descendant's dispatch commit never claims this branch's role: the
    // pattern is anchored on the exact `[<agent-id>]` tail.
    commit(wt, "dispatch: worker [p1-c1-w9]", "x.md", "x");
    let child = state(wt, "p1-c1", now_of(wt), false, &RealGit::new()).unwrap();
    assert!(child.is_compactor);
    let worker = state(wt, "p1-c1-w9", now_of(wt), false, &RealGit::new()).unwrap();
    assert!(!worker.is_compactor);
}

#[test]
fn state_falls_back_to_the_root_when_a_branch_has_no_dispatch_commit() {
    // The general path with empty inputs, not a bootstrap special case:
    // a tree carrying no commit that names this agent measures from the
    // branch root, exactly as before.
    let dir = TempDir::new().unwrap();
    let wt = dir.path();
    init(wt);
    commit(wt, "root", "a.txt", "1");
    commit(wt, "step", "b.txt", "2");
    let s = state(wt, "nobody", now_of(wt), false, &RealGit::new()).unwrap();
    assert_eq!(s.commits_since_checkpoint, 2);
    assert!(!s.is_compactor);
}

// ---- per-op git failures, via a stub -----------------------------------
//
// Real git can only fail wholesale (the test above); each later derivation
// step's `op` tag needs a git that fails at exactly that step. The stub
// answers every capture except the one whose args contain `fail_on`.

struct FailOn(&'static str);

impl GitRunner for FailOn {
    fn run(&self, _dest: &Path, _args: &[&str]) -> std::io::Result<()> {
        unreachable!("checkpoint derivation only captures")
    }
    fn run_capture(&self, _dest: &Path, args: &[&str]) -> std::io::Result<String> {
        if args.iter().any(|a| a.contains(self.0)) {
            return Err(std::io::Error::other("stub git failure"));
        }
        // Benign answers: no checkpoint landed (empty grep), one commit,
        // a root sha, epoch second 100.
        Ok(match args {
            a if a.contains(&"--grep") => String::new(),
            a if a.contains(&"--count") => "1".to_string(),
            a if a.contains(&"--max-parents=0") => "r00t".to_string(),
            _ => "100".to_string(),
        })
    }
}

fn op_of(err: Error) -> &'static str {
    match err {
        Error::Git { op, .. } => op,
        other => panic!("expected Error::Git, got {other:?}"),
    }
}

#[test]
fn state_tags_a_commit_count_failure_with_its_op() {
    let dir = TempDir::new().unwrap();
    let err = state(dir.path(), "p1", 0, false, &FailOn("--count")).unwrap_err();
    assert_eq!(op_of(err), "checkpoint rev-list count");
}

#[test]
fn state_tags_a_commit_time_failure_with_its_op() {
    let dir = TempDir::new().unwrap();
    let err = state(dir.path(), "p1", 0, false, &FailOn("%ct")).unwrap_err();
    assert_eq!(op_of(err), "checkpoint commit time");
}

#[test]
fn state_tags_a_root_lookup_failure_with_its_op() {
    let dir = TempDir::new().unwrap();
    let err = state(dir.path(), "p1", 0, false, &FailOn("--max-parents=0")).unwrap_err();
    assert_eq!(op_of(err), "checkpoint root rev-list");
}

#[test]
fn state_tags_an_in_flight_enumeration_failure_with_its_op() {
    // bl-b9f0: the in-flight read is the registry query (§8's enumeration
    // seam, asked from the worktree), and a failure there is loud like
    // every other step of the derivation rather than a silent `false`.
    let dir = TempDir::new().unwrap();
    let err = state(dir.path(), "p1", 0, false, &FailOn("for-each-ref")).unwrap_err();
    assert_eq!(op_of(err), "checkpoint in-flight for-each-ref");
}

#[test]
fn state_carries_the_branchs_last_usage() {
    // The window trigger's numerator and denominator both ride the
    // read-state tree (§5.1), so the derivation reads them from the same
    // worktree the clock is measured in — no second input.
    let dir = TempDir::new().unwrap();
    let wt = dir.path();
    init(wt);
    commit(wt, "step 001: dispatch [p1]", "goal.md", "g");
    commit(
        wt,
        "transcript 001: m [p1]",
        "messages/001-m.json",
        r#"{"content":[],"usage":{"input_tokens":40,"context_window":100}}"#,
    );
    let s = state(wt, "p1", now_of(wt), false, &RealGit::new()).unwrap();
    assert_eq!(
        s.last_usage,
        Some(LastUsage {
            prompt_tokens: 40,
            context_window: Some(100),
            model: "m".into(),
        })
    );
}
