//! The spawn's failure arms: every git step of
//! `subagent::spawn_subagent_branch` surfaces in its own voice rather
//! than as an anonymous git error, and a colliding worktree path
//! surfaces as I/O. Split from [`super::tests`] for the per-file line
//! cap; the stub and the request fixture live there.

use super::tests::{EMPTY_GRANT, StubGit, req, tmpdir};
use super::*;

#[test]
fn surfaces_worktree_add_failure() {
    let parent_dir = tmpdir();
    let sub_dir = tmpdir();
    let git = StubGit::failing_at(0);
    let err =
        spawn_subagent_branch(&req(parent_dir.path(), sub_dir.path(), None), &git).unwrap_err();
    assert!(
        matches!(
            err,
            Error::Git {
                op: "worktree add",
                ..
            }
        ),
        "got {err:?}"
    );
}

#[test]
fn surfaces_control_rm_failure() {
    let parent_dir = tmpdir();
    let sub_dir = tmpdir();
    let git = StubGit::failing_at(1);
    let err = spawn_subagent_branch(
        &req(parent_dir.path(), sub_dir.path(), Some("soul\n")),
        &git,
    )
    .unwrap_err();
    assert!(
        matches!(
            err,
            Error::Git {
                op: "rm control files",
                ..
            }
        ),
        "got {err:?}"
    );
}

#[test]
fn surfaces_dialog_prune_failure() {
    let parent_dir = tmpdir();
    let sub_dir = tmpdir();
    let git = StubGit::failing_at(5);
    let err = spawn_subagent_branch(
        &req(parent_dir.path(), sub_dir.path(), Some("soul\n")),
        &git,
    )
    .unwrap_err();
    assert!(
        matches!(
            err,
            Error::Git {
                op: "rm inherited dialog",
                ..
            }
        ),
        "got {err:?}"
    );
}

#[test]
fn surfaces_add_failure() {
    let parent_dir = tmpdir();
    let sub_dir = tmpdir();
    let git = StubGit::failing_at(6);
    let err = spawn_subagent_branch(
        &req(parent_dir.path(), sub_dir.path(), Some("soul\n")),
        &git,
    )
    .unwrap_err();
    assert!(matches!(err, Error::Git { op: "add", .. }), "got {err:?}");
}

#[test]
fn surfaces_commit_failure() {
    let parent_dir = tmpdir();
    let sub_dir = tmpdir();
    let git = StubGit::failing_at(7);
    let err =
        spawn_subagent_branch(&req(parent_dir.path(), sub_dir.path(), None), &git).unwrap_err();
    assert!(
        matches!(err, Error::Git { op: "commit", .. }),
        "got {err:?}"
    );
}

#[test]
fn surfaces_io_failure_when_sub_worktree_is_a_file() {
    // Production `git worktree add` creates the directory; in the
    // stub-git test path we lean on `create_dir_all` for the same.
    // If the target path already exists as a regular file (e.g.
    // because of a stale remnant), that fails — the helper surfaces
    // the io::Error unchanged via the Error::Io conversion.
    let parent_dir = tmpdir();
    let sub_wt = parent_dir.path().join("collision");
    std::fs::write(&sub_wt, b"existing file").unwrap();
    let git = StubGit::ok();
    let r = SpawnRequest {
        parent_worktree: parent_dir.path(),
        sub_branch: "p1-x",
        sub_worktree: &sub_wt,
        fork_point: "agents/p1",
        goal_text: "g",
        soul_text: None,
        name: None,
        pins: crate::prompt::PinnedDocs::none(),
        grant: &EMPTY_GRANT,
        commit_subject: "x",
    };
    let err = spawn_subagent_branch(&r, &git).unwrap_err();
    assert!(matches!(err, Error::Io(_)), "got {err:?}");
}

#[test]
fn surfaces_name_settle_failure() {
    // The trim's sixth part (§2.3): staging the settled `name` fails,
    // and the dispatch commit reports it in its own voice rather than
    // as an anonymous git error (2 and 3 are the facts cut's probe and
    // checkout, which the stub answers).
    let parent_dir = tmpdir();
    let sub_dir = tmpdir();
    let git = StubGit::failing_at(4);
    let err =
        spawn_subagent_branch(&req(parent_dir.path(), sub_dir.path(), None), &git).unwrap_err();
    assert!(
        matches!(
            err,
            Error::Git {
                op: "settle the agent name",
                ..
            }
        ),
        "got {err:?}"
    );
}
