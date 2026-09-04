//! The reviewer's landing over real git (`docs/DESIGN_LEARNING_LOOP.md`
//! §3, §6): a reviewer child forked through the ordinary dispatch, its
//! edits committed on its own branch, its return interpreted by the §6
//! binding interpreter. The harness is [`super::super::tests`]'s — the
//! same real workspace every other child-result seam is proved against.
//! The refusals — every way a proposal is declined whole — are
//! [`refusals`].

mod refusals;

use super::super::tests::{Fx, returned_child, workflow};
use super::super::{ChildResult, interpret_pending};
use super::{Staged, mint};
use crate::prompt::inbox::{self, Epitaph};
use crate::prompt::role;
use crate::template::{GitRunner, RealGit};
use crate::workspace::proposal::proposal_ref;
use crate::workspace::{agent_ref, config_ref, fixture, repo_git};
use std::path::Path;

/// The binding the learning loop ships (`workflows/learning-loop.yaml`).
pub(super) const STAGE: &str = "events:\n  reviewer_return:\n    - stage_proposal\n";

/// A `SKILL.md` the descriptions snapshot's parser accepts.
pub(super) fn manifest(name: &str, body: &str) -> String {
    format!("---\nname: {name}\ndescription: d\n---\n{body}")
}

/// `git rev-parse <rev>` in the workspace's bare repo, or `None` when
/// the ref does not exist — how every assertion below asks whether a
/// proposal was written.
pub(super) fn rev(ws: &Path, rev: &str) -> Option<String> {
    RealGit::new()
        .run_capture(&repo_git(ws), &["rev-parse", "--verify", rev])
        .ok()
        .map(|s| s.trim().to_string())
}

/// A workspace whose default lineage carries one workspace skill, plus a
/// dispatching branch with a worktree. Returns `(holder, ws, worktree)`.
pub(super) fn workspace_with_a_skill(
    parent: &str,
) -> (tempfile::TempDir, std::path::PathBuf, std::path::PathBuf) {
    let (h, ws) = fixture::workspace();
    let wt = fixture::spawn_root(&ws, parent);
    fixture::amend_config(
        &ws,
        &[("skills/notes/SKILL.md", &manifest("notes", "the lesson"))],
    );
    (h, ws, wt)
}

#[test]
fn a_reviewers_edit_lands_as_one_proposal_commit_off_the_tip() {
    let parent = "20260101-p1";
    let (_h, ws, wt) = workspace_with_a_skill(parent);
    let fx = Fx::new();
    let patched = manifest("notes", "the lesson, corrected");
    let child = returned_child(
        &ws,
        parent,
        "reviewer",
        "review it",
        ("skills/notes/SKILL.md", patched.as_str()),
        &fx,
    );
    interpret_pending(&ws, parent, &wt, &workflow(STAGE), &fx.deps()).unwrap();

    let target = proposal_ref(&child);
    let staged = rev(&ws, &target).expect("the proposal branch exists");
    // Parented on the followed config commit the reviewer read (§3).
    assert_eq!(
        rev(&ws, &format!("{target}^")),
        rev(&ws, &config_ref("default")),
        "the parent is the lineage tip"
    );
    let git = RealGit::new();
    let repo = repo_git(&ws);
    assert_eq!(
        git.run_capture(&repo, &["show", &format!("{staged}:skills/notes/SKILL.md")])
            .unwrap()
            .trim(),
        patched.trim(),
        "the diff is the reviewer's edit"
    );
    assert_eq!(
        git.run_capture(&repo, &["log", "-1", "--format=%B", &staged])
            .unwrap()
            .trim(),
        "done",
        "the message is the reviewer's terminal response"
    );
    // Consumed, never delivered: no transcript entry, no inbox message.
    assert!(
        !wt.join("messages").exists(),
        "the dispatcher's transcript carries no review"
    );
    assert!(
        !inbox::inbox_dir(&ws, parent)
            .join(format!("{child}-001.md"))
            .exists(),
        "the return is consumed"
    );
    // The lineage did not move: acceptance is the operator's act.
    assert_ne!(rev(&ws, &config_ref("default")), Some(staged));
}

#[test]
fn a_pass_that_changes_nothing_declines_and_leaves_no_ref() {
    // The mint's own contract, asked directly: an empty edit is the
    // authoring routine's *declined pass*, which deletes the branch it
    // created. The landing above never reaches it (an empty diff is
    // answered before anything is materialized), and this is why the
    // arm is not a second empty-inputs rule.
    let parent = "20260101-p8";
    let (_h, ws, wt) = workspace_with_a_skill(parent);
    let fx = Fx::new();
    let child = returned_child(
        &ws,
        parent,
        "reviewer",
        "review it",
        ("messages/900-tool.json", "[]"),
        &fx,
    );
    let git = RealGit::new();
    let terminal = git
        .run_capture(&wt, &["rev-parse", &agent_ref(&child)])
        .unwrap();
    let terminal = terminal.trim().to_string();
    let founding = role::founding_sha(&wt, &terminal, &child, &git)
        .unwrap()
        .expect("the child has a dispatch commit");
    let tip = rev(&ws, &config_ref("default")).unwrap();
    let cr = ChildResult {
        child_id: child.clone(),
        terminal_ref: terminal,
        epitaph: Epitaph::FinalResponse.as_str().to_string(),
        response: Some("nothing to propose".into()),
        path: inbox::inbox_dir(&ws, parent).join(format!("{child}-001.md")),
    };
    let staged = mint(&ws, &wt, &cr, &founding, &tip, &fx.deps()).unwrap();
    assert!(matches!(staged, Staged::Empty), "a declined pass");
    assert_eq!(rev(&ws, &proposal_ref(&child)), None);
}

/// A `GitRunner` that cannot answer — the shape of a workspace whose
/// repository is unreachable.
struct NoGit;
impl GitRunner for NoGit {
    fn run(&self, _d: &Path, _a: &[&str]) -> std::io::Result<()> {
        Err(std::io::Error::other("git boom"))
    }
    fn run_capture(&self, _d: &Path, _a: &[&str]) -> std::io::Result<String> {
        Err(std::io::Error::other("git boom"))
    }
}

#[test]
fn an_unanswerable_lineage_surfaces_as_a_git_error() {
    // The one failure the freshness check has: the tip cannot be
    // derived at all, which is not a stale proposal and must not read
    // as one.
    let err = super::lineage_tip(Path::new("/nonexistent"), "p1-c2", &NoGit).unwrap_err();
    assert!(
        matches!(
            err,
            crate::prompt::Error::Git {
                op: "proposal lineage tip",
                ..
            }
        ),
        "{err:?}"
    );
}

#[test]
fn an_unbound_reviewer_return_still_stages_and_never_delivers() {
    // The baseline default (§6, `child_actions`): `reviewer_return` binds
    // `stage_proposal` for the reason `compactor_return` binds
    // `land_compaction`. A review that fell through to `deliver_result`
    // would put itself in the reviewed agent's own context, which is the
    // one thing the whole design forbids (§2 *Never on the critical
    // path*) — so an unbound event stages rather than delivers.
    let parent = "20260101-p10";
    let (_h, ws, wt) = workspace_with_a_skill(parent);
    let fx = Fx::new();
    let child = returned_child(
        &ws,
        parent,
        "reviewer",
        "review it",
        (
            "skills/notes/SKILL.md",
            &manifest("notes", "unbound but staged"),
        ),
        &fx,
    );
    interpret_pending(&ws, parent, &wt, &workflow("events: {}\n"), &fx.deps()).unwrap();
    assert!(
        rev(&ws, &proposal_ref(&child)).is_some(),
        "the default binding staged it"
    );
    assert!(!wt.join("messages").exists(), "and delivered nothing");
}
