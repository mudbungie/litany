//! Unit tests for `subagent::spawn_subagent_branch`.
//!
//! Lives in a sibling file rather than an inline `mod tests` so the
//! production module stays under the 300-line repo cap.

use super::*;
use std::cell::RefCell;
use std::io;
use std::path::PathBuf;

#[derive(Default)]
pub(super) struct StubGit {
    runs: RefCell<Vec<(PathBuf, Vec<String>)>>,
    fail_at: Option<usize>,
}
impl StubGit {
    pub(super) fn ok() -> Self {
        Self::default()
    }
    pub(super) fn failing_at(idx: usize) -> Self {
        Self {
            fail_at: Some(idx),
            ..Self::default()
        }
    }
}
impl GitRunner for StubGit {
    fn run(&self, dest: &Path, args: &[&str]) -> io::Result<()> {
        let mut runs = self.runs.borrow_mut();
        let idx = runs.len();
        runs.push((
            dest.to_path_buf(),
            args.iter().map(|s| (*s).to_owned()).collect(),
        ));
        if self.fail_at == Some(idx) {
            Err(io::Error::other(format!("stub git fail at {idx}")))
        } else {
            Ok(())
        }
    }
    fn run_capture(&self, _dest: &Path, _args: &[&str]) -> io::Result<String> {
        // The subagent spawn helper never calls run_capture — every git
        // op it issues is fire-and-forget, the trim's skill-body drop
        // (`step_commit::skill_bodies`, whose enumeration is a worktree
        // read) included. The trait still requires an impl; panicking
        // documents the assumption and tarpaulin's `ignore-panics`
        // excludes the branch from the coverage floor.
        unreachable!("spawn_subagent_branch never issues capturing git ops")
    }
}

/// A grant of nothing against the stub's fixed config commit — the
/// compactor's shape (§2.7), and all these stub trees can honour.
pub(super) const EMPTY_GRANT: crate::prompt::dispatch::Grant<'static> =
    crate::prompt::dispatch::Grant {
        role: "worker",
        tools: &[],
        config_commit: "c0ffee",
    };

pub(super) fn tmpdir() -> tempfile::TempDir {
    tempfile::TempDir::new().unwrap()
}

pub(super) fn req<'a>(
    parent_wt: &'a Path,
    sub_wt: &'a Path,
    soul: Option<&'a str>,
) -> SpawnRequest<'a> {
    SpawnRequest {
        parent_worktree: parent_wt,
        sub_branch: "p1-ct-2-deadbeef",
        sub_worktree: sub_wt,
        fork_point: "agents/p1",
        goal_text: "do the thing\n",
        soul_text: soul,
        pins: crate::prompt::PinnedDocs::none(),
        name: None,
        // The stub worktrees carry no `descriptions/**` and the grant is
        // empty, so the descriptor half of the trim is a no-op here
        // (§3.3) — exercised on its own in
        // `dispatch::step_commit::descriptors::tests`.
        grant: &EMPTY_GRANT,
        commit_subject: "dispatch: worker [p1-ct-2-deadbeef]",
    }
}

#[test]
fn writes_goal_and_soul_when_soul_present() {
    let parent_dir = tmpdir();
    let sub_dir = tmpdir();
    let parent_wt = parent_dir.path();
    let sub_wt = sub_dir.path();
    let git = StubGit::ok();

    spawn_subagent_branch(&req(parent_wt, sub_wt, Some("you are worker\n")), &git).unwrap();

    let runs = git.runs.borrow();
    // 0: worktree add (in parent worktree) — ids map to agents/* refs
    // at the git boundary (§2.3).
    assert_eq!(runs[0].0, parent_wt);
    assert_eq!(
        runs[0].1[..4],
        ["worktree", "add", "-b", "agents/p1-ct-2-deadbeef"]
    );
    assert_eq!(runs[0].1[4], sub_wt.to_string_lossy().to_string());
    assert_eq!(runs[0].1[5], "agents/p1");
    // 1: control-file removal (total, --ignore-unmatch; §2.3 step 2)
    assert_eq!(runs[1].0, sub_wt);
    assert_eq!(runs[1].1[..5], ["rm", "-r", "-q", "--ignore-unmatch", "--"]);
    // 2-3: the facts cut — the lineage's durable memory is derived
    // from the governing config commit at every fork (§5.5). The stub
    // answers every command, so the existence probe reads as carried
    // and the checkout follows.
    assert_eq!(runs[2].1, vec!["cat-file", "-e", "c0ffee:facts.md"]);
    assert_eq!(runs[3].1, vec!["checkout", "c0ffee", "--", "facts.md"]);
    // 4: stage the settled name — the trim's sixth part (§2.3)
    assert_eq!(runs[4].0, sub_wt);
    assert_eq!(runs[4].1, vec!["add", "name"]);
    // 5: the inherited-dialog prune — a child's opening context is
    // never its dispatcher's conversation (§2.2, bl-5a36)
    assert_eq!(runs[5].0, sub_wt);
    assert_eq!(
        runs[5].1,
        vec![
            "rm",
            "-r",
            "-q",
            "--ignore-unmatch",
            "--",
            "messages",
            "summary",
            "skills"
        ]
    );
    // 6: add goal.md soul.md (in sub worktree)
    assert_eq!(runs[6].0, sub_wt);
    assert_eq!(runs[6].1, vec!["add", "goal.md", "soul.md"]);
    // 7: commit (in sub worktree)
    assert_eq!(runs[7].0, sub_wt);
    assert_eq!(runs[7].1[0], "commit");
    assert_eq!(runs[7].1[2], "dispatch: worker [p1-ct-2-deadbeef]");

    assert_eq!(
        std::fs::read_to_string(sub_wt.join("goal.md")).unwrap(),
        "do the thing\n"
    );
    assert_eq!(
        std::fs::read_to_string(sub_wt.join("soul.md")).unwrap(),
        "you are worker\n"
    );
    // An unnamed child still carries the fact's file, empty (§2.3 —
    // one shape, so a fork never inherits its parent's name).
    assert_eq!(std::fs::read_to_string(sub_wt.join("name")).unwrap(), "");
}

#[test]
fn writes_only_goal_when_soul_is_none() {
    let parent_dir = tmpdir();
    let sub_dir = tmpdir();
    let git = StubGit::ok();

    spawn_subagent_branch(&req(parent_dir.path(), sub_dir.path(), None), &git).unwrap();

    let runs = git.runs.borrow();
    // The stage step adds only goal.md.
    assert_eq!(runs[6].1, vec!["add", "goal.md"]);
    assert!(
        !sub_dir.path().join("soul.md").exists(),
        "soul.md should not be written"
    );
}
