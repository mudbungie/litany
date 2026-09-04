//! The checkpoint forks the **reviewer beside the compactor**
//! (`docs/DESIGN_LEARNING_LOOP.md` §2, ARCH §2.2, §2.7): one due clock,
//! one compaction point, every dispatch the event binds. Split from
//! [`flush_clock`](super::flush_clock) to hold the per-file line cap; the
//! shared real-git harness lives in [`super`].

use super::super::run_flush;
use super::{Fx, workflow};
use crate::template::{GitRunner, RealGit};
use crate::workspace::{agent_worktree, fixture};
use std::path::Path;

/// The learning loop's `worker_flush` binding (`workflows/learning-loop.yaml`)
/// over a clock due at every commit.
const BOTH: &str = "events:\n  worker_flush:\n    - dispatch(compactor)\n    - dispatch(reviewer)\ncompaction:\n  intermediate:\n    trigger: every_n_commits\n    n: 1\n";

/// A `SKILL.md` the descriptions snapshot's parser accepts — the shape
/// `load_skill`'s workspace-skill fixtures use.
fn manifest(name: &str, body: &str) -> String {
    format!("---\nname: {name}\ndescription: d\n---\n{body}")
}

/// A dispatching branch with a transcript entry and a prior summary —
/// the span a checkpoint fires on. Returns its worktree.
fn branch_with_a_span(ws: &Path, parent: &str) -> std::path::PathBuf {
    let wt = fixture::spawn_root(ws, parent);
    let git = RealGit::new();
    std::fs::create_dir_all(wt.join("messages")).unwrap();
    std::fs::create_dir_all(wt.join("summary")).unwrap();
    std::fs::write(wt.join("messages/001-user.md"), "the user said a thing\n").unwrap();
    std::fs::write(wt.join("summary/001.md"), "an earlier digest\n").unwrap();
    git.run(&wt, &["add", "-A"]).unwrap();
    git.run(&wt, &["commit", "-m", "span"]).unwrap();
    wt
}

/// `<child>~1` — the commit the child's dispatch commit is parented on.
fn fork_parent(wt: &Path, child: &str) -> String {
    RealGit::new()
        .run_capture(
            wt,
            &[
                "rev-parse",
                &format!("{}~1", crate::workspace::agent_ref(child)),
            ],
        )
        .unwrap()
}

#[test]
fn one_due_clock_forks_a_compactor_and_a_reviewer_off_one_point() {
    // §2 there: the reviewer rides the compaction checkpoint. Both
    // children fork off the SAME commit — the evidence the reviewer
    // inspects is exactly the evidence the compactor is about to squash
    // — and both keep the inherited dialog, which a worker child does
    // not (ARCH §2.2, the three principled keepers).
    let (_h, ws) = fixture::workspace();
    let parent = "20260101-r1";
    let wt = branch_with_a_span(&ws, parent);
    fixture::amend_config(
        &ws,
        &[(
            "skills/notes/SKILL.md",
            manifest("notes", "the lesson").as_str(),
        )],
    );
    let fx = Fx::new();
    run_flush(&ws, parent, &wt, &workflow(BOTH), &fx.deps()).unwrap();

    let launched = fx.launcher.launched.borrow().clone();
    assert_eq!(launched.len(), 2, "one clock, two forks: {launched:?}");
    let point = RealGit::new()
        .run_capture(&wt, &["rev-parse", "HEAD"])
        .unwrap();
    for child in &launched {
        assert_eq!(
            fork_parent(&wt, child),
            point,
            "{child} forked off the point"
        );
    }
    // The reviewer is the second binding's child; the compactor the
    // first's. Both keep the dialog.
    let reviewer = agent_worktree(&ws, &launched[1]);
    assert!(reviewer.join("messages/001-user.md").exists());
    assert!(reviewer.join("summary/001.md").exists());
    assert!(
        agent_worktree(&ws, &launched[0])
            .join("messages/001-user.md")
            .exists(),
        "the compactor keeps it too"
    );
    // …and the reviewer's tree carries the followed config commit's
    // workspace skill, which the fork point never had (it was authored
    // after the branch forked — follow-the-tip, ARCH §2.2).
    assert_eq!(
        std::fs::read_to_string(reviewer.join("skills/notes/SKILL.md")).unwrap(),
        manifest("notes", "the lesson"),
    );
    assert!(
        !wt.join("skills/notes/SKILL.md").exists(),
        "not the parent's"
    );
}

#[test]
fn a_worker_child_forked_at_a_checkpoint_carries_no_dialog_and_no_bodies() {
    // The other side of the keeper rule: an ordinary child's opening
    // context is its own goal and soul, never its dispatcher's dialog,
    // and a lineage's skill body is not context until elected (§3 there).
    use crate::prompt::child_dispatch::{ChildDispatchRequest, run as dispatch_child};
    let (_h, ws) = fixture::workspace();
    let parent = "20260101-r2";
    let wt = branch_with_a_span(&ws, parent);
    fixture::amend_config(
        &ws,
        &[(
            "skills/notes/SKILL.md",
            manifest("notes", "the lesson").as_str(),
        )],
    );
    let fx = Fx::new();
    let req = ChildDispatchRequest {
        repo: &ws,
        parent_branch: parent,
        parent_worktree: &wt,
        role: "worker",
        goal: "do it",
        name: None,
        fork_point: None,
        cwd: None,
        pins: crate::prompt::PinnedDocs::none(),
    };
    let child = dispatch_child(
        &req,
        &fx.git,
        &fx.clock,
        &fx.id,
        &fx.launcher,
        crate::workspace::agent_name::mint::test_rng(),
    )
    .unwrap();
    let cwt = agent_worktree(&ws, &child);
    assert!(!cwt.join("messages/001-user.md").exists());
    assert!(!cwt.join("summary/001.md").exists());
    assert!(!cwt.join("skills/notes/SKILL.md").exists());
}

#[test]
fn the_reviewers_read_is_the_config_commits_body_not_the_forked_copy() {
    // "A fresh read precedes every write by construction" (§2 there):
    // the parent carries an older elected copy of the same name, and the
    // reviewer's tree carries the followed commit's — so a proposal is
    // never a patch against a body its parent had gone stale on.
    let (_h, ws) = fixture::workspace();
    let parent = "20260101-r3";
    let wt = branch_with_a_span(&ws, parent);
    let git = RealGit::new();
    std::fs::create_dir_all(wt.join("skills/notes")).unwrap();
    std::fs::write(
        wt.join("skills/notes/SKILL.md"),
        manifest("notes", "the stale lesson"),
    )
    .unwrap();
    git.run(&wt, &["add", "-A"]).unwrap();
    git.run(&wt, &["commit", "-m", "elected"]).unwrap();
    fixture::amend_config(
        &ws,
        &[(
            "skills/notes/SKILL.md",
            manifest("notes", "the current lesson").as_str(),
        )],
    );
    let fx = Fx::new();
    run_flush(&ws, parent, &wt, &workflow(BOTH), &fx.deps()).unwrap();

    let reviewer = agent_worktree(&ws, &fx.launcher.launched.borrow()[1]);
    assert_eq!(
        std::fs::read_to_string(reviewer.join("skills/notes/SKILL.md")).unwrap(),
        manifest("notes", "the current lesson"),
    );
}

#[test]
fn the_reviewer_carries_the_facts_document_when_the_lineage_has_one() {
    // `docs/DESIGN_LEARNING_LOOP.md` §4: the facts document is the
    // proposal's second admitted class, so it is read in beside the
    // skills. Its cut and cap are `docs/DESIGN_CONTEXT_ECONOMY.md` §3's.
    let (_h, ws) = fixture::workspace();
    let parent = "20260101-r4";
    let wt = branch_with_a_span(&ws, parent);
    fixture::amend_config(&ws, &[("facts.md", "the box has no network\n")]);
    let fx = Fx::new();
    run_flush(&ws, parent, &wt, &workflow(BOTH), &fx.deps()).unwrap();

    let reviewer = agent_worktree(&ws, &fx.launcher.launched.borrow()[1]);
    assert_eq!(
        std::fs::read_to_string(reviewer.join("facts.md")).unwrap(),
        "the box has no network\n"
    );
}

#[test]
fn a_lineage_carrying_neither_class_checks_out_nothing() {
    // The general path with empty inputs — every workspace that has
    // authored no workspace skill, and (until that design's writer
    // lands) every workspace at all for the facts document. The fork
    // still happens; it simply carries neither.
    let (_h, ws) = fixture::workspace();
    let parent = "20260101-r5";
    let wt = branch_with_a_span(&ws, parent);
    let fx = Fx::new();
    run_flush(&ws, parent, &wt, &workflow(BOTH), &fx.deps()).unwrap();

    let reviewer = agent_worktree(&ws, &fx.launcher.launched.borrow()[1]);
    assert!(!reviewer.join("facts.md").exists());
    assert!(!reviewer.join("skills").exists());
    assert!(
        reviewer.join("messages/001-user.md").exists(),
        "still forked"
    );
}

#[test]
fn a_checkpoint_dispatch_of_a_role_with_no_goal_is_declined() {
    // The harness mints a checkpoint goal for the two roles the design
    // gives it one for. A `worker_flush: dispatch(worker)` names a role
    // the harness has nothing to instruct, so it is declined loudly
    // rather than forking a child with nothing to do.
    let (_h, ws) = fixture::workspace();
    let parent = "20260101-r6";
    let wt = branch_with_a_span(&ws, parent);
    let fx = Fx::new();
    let wf = workflow(
        "events:\n  worker_flush:\n    - dispatch(worker)\ncompaction:\n  intermediate:\n    trigger: every_n_commits\n    n: 1\n",
    );
    let err = run_flush(&ws, parent, &wt, &wf, &fx.deps()).unwrap_err();
    assert!(
        matches!(err, crate::prompt::Error::ActionUnsupported { ref event, .. } if event == &"worker_flush"),
        "{err:?}"
    );
    assert!(fx.launcher.launched.borrow().is_empty());
}
