//! The operator's acts over a real workspace: fresh and stale derived
//! from refs alone, acceptance as a compare-and-swap fast-forward, and
//! rejection as a branch deletion that touches nothing else.

use super::*;
use crate::template::RealGit;
use crate::workspace::{config_ref, fixture, proposal::proposal_ref, repo_git};
use std::path::PathBuf;
use tempfile::TempDir;

/// A workspace with one staged proposal on the default lineage: a
/// commit that adds a workspace skill, cut at the lineage head exactly
/// as `stage_proposal` cuts it. Returns `(holder, ws, parent sha)`.
fn workspace_with_a_proposal(id: &str) -> (TempDir, PathBuf, String) {
    let (h, ws) = fixture::workspace();
    let git = RealGit::new();
    let repo = repo_git(&ws);
    let parent = git
        .run_capture(&repo, &["rev-parse", &config_ref("default")])
        .unwrap();
    let parent = parent.trim().to_string();
    let scratch = h.path().join("mint");
    let scratch_s = scratch.to_string_lossy().into_owned();
    git.run(
        &repo,
        &[
            "worktree",
            "add",
            "-b",
            &proposal_ref(id),
            &scratch_s,
            &parent,
        ],
    )
    .unwrap();
    std::fs::create_dir_all(scratch.join("skills/notes")).unwrap();
    std::fs::write(
        scratch.join("skills/notes/SKILL.md"),
        "---\nname: notes\ndescription: d\n---\nthe lesson\n",
    )
    .unwrap();
    git.run(&scratch, &["add", "-A"]).unwrap();
    git.run(&scratch, &["commit", "-m", "notes: record the lesson"])
        .unwrap();
    git.run(&repo, &["worktree", "remove", "--force", &scratch_s])
        .unwrap();
    (h, ws, parent)
}

fn head(ws: &Path, r: &str) -> Option<String> {
    RealGit::new()
        .run_capture(&repo_git(ws), &["rev-parse", "--verify", r])
        .ok()
        .map(|s| s.trim().to_string())
}

#[test]
fn a_proposal_lists_fresh_against_the_lineage_it_stands_on() {
    let (_h, ws, parent) = workspace_with_a_proposal("a1-r1");
    let rows = list(&ws, &RealGit::new()).unwrap();
    assert_eq!(rows.len(), 1);
    let row = &rows[0];
    assert_eq!(row.id, "a1-r1");
    assert_eq!(row.lineages, vec!["default".to_string()]);
    assert!(row.fresh, "the parent is still the head");
    assert_eq!(row.parent, short(&parent));
    assert_eq!(row.subject, "notes: record the lesson");
    assert!(row.diffstat.contains("1 file changed"), "{}", row.diffstat);
}

#[test]
fn a_config_advance_makes_the_same_proposal_stale_with_no_field_rewritten() {
    let (_h, ws, _parent) = workspace_with_a_proposal("a1-r2");
    fixture::amend_config(&ws, &[("souls/worker.md", "a new soul")]);
    let rows = list(&ws, &RealGit::new()).unwrap();
    assert!(!rows[0].fresh, "the head moved out from under it");
    assert!(rows[0].lineages.is_empty(), "no lineage stands there now");
}

#[test]
fn accept_fast_forwards_the_lineage_and_deletes_the_branch() {
    let (_h, ws, _parent) = workspace_with_a_proposal("a1-r3");
    let git = RealGit::new();
    let staged = head(&ws, &proposal_ref("a1-r3")).unwrap();
    let line = accept(&ws, "a1-r3", &git).unwrap();
    assert!(line.starts_with("accepted a1-r3: config/default"), "{line}");
    assert_eq!(
        head(&ws, &config_ref("default")),
        Some(staged),
        "the lineage stands at the proposal"
    );
    assert_eq!(
        head(&ws, &proposal_ref("a1-r3")),
        None,
        "the branch is gone"
    );
}

#[test]
fn accept_on_a_stale_proposal_refuses_naming_the_tip_and_moves_nothing() {
    let (_h, ws, _parent) = workspace_with_a_proposal("a1-r4");
    fixture::amend_config(&ws, &[("souls/worker.md", "a new soul")]);
    let tip = head(&ws, &config_ref("default")).unwrap();
    let err = accept(&ws, "a1-r4", &RealGit::new()).unwrap_err();
    let rendered = err.to_string();
    assert!(matches!(err, Error::Stale { .. }), "{rendered}");
    assert!(rendered.contains(&short(&tip)), "{rendered}");
    assert_eq!(head(&ws, &config_ref("default")), Some(tip), "unmoved");
    assert!(
        head(&ws, &proposal_ref("a1-r4")).is_some(),
        "and unrejected"
    );
}

#[test]
fn accept_declines_a_parent_two_lineages_stand_on() {
    let (_h, ws, parent) = workspace_with_a_proposal("a1-r5");
    RealGit::new()
        .run(&repo_git(&ws), &["branch", &config_ref("variant"), &parent])
        .unwrap();
    let err = accept(&ws, "a1-r5", &RealGit::new()).unwrap_err();
    assert!(matches!(err, Error::Ambiguous { .. }), "{err}");
    assert!(err.to_string().contains("variant"), "{err}");
    assert_eq!(head(&ws, &config_ref("default")), Some(parent), "unmoved");
}

#[test]
fn reject_deletes_only_the_proposal() {
    let (_h, ws, parent) = workspace_with_a_proposal("a1-r6");
    let line = reject(&ws, "a1-r6", &RealGit::new()).unwrap();
    assert!(line.starts_with("rejected a1-r6"), "{line}");
    assert_eq!(head(&ws, &proposal_ref("a1-r6")), None);
    assert_eq!(head(&ws, &config_ref("default")), Some(parent));
}

#[test]
fn show_renders_the_message_and_the_whole_diff() {
    let (_h, ws, _parent) = workspace_with_a_proposal("a1-r7");
    let out = show(&ws, "a1-r7", &RealGit::new()).unwrap();
    assert!(out.contains("notes: record the lesson"), "{out}");
    assert!(out.contains("+the lesson"), "{out}");
}

#[test]
fn an_id_naming_no_proposal_is_declined_with_the_ones_there_are() {
    let (_h, ws, _parent) = workspace_with_a_proposal("a1-r8");
    let err = show(&ws, "a1-r9", &RealGit::new()).unwrap_err();
    assert!(matches!(err, Error::Unknown { .. }), "{err}");
    assert!(err.to_string().contains("a1-r8"), "{err}");
}

#[test]
fn a_workspace_with_no_proposals_lists_none() {
    let (_h, ws) = fixture::workspace();
    assert!(list(&ws, &RealGit::new()).unwrap().is_empty());
}

#[test]
fn a_path_that_is_no_workspace_is_declined_before_any_ref_is_read() {
    let dir = TempDir::new().unwrap();
    let err = list(dir.path(), &RealGit::new()).unwrap_err();
    assert!(matches!(err, Error::Layout(_)), "{err}");
}

/// A `GitRunner` that cannot answer — the two ref enumerations' only
/// failure mode, and the one arm no real repository produces on demand.
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
fn an_unreadable_proposal_namespace_is_a_git_error_not_an_empty_listing() {
    // The distinction the listing must keep: "no proposal is staged" and
    // "the registry could not be read" are different answers, and only
    // one of them is empty.
    let (_h, ws) = fixture::workspace();
    let err = list(&ws, &NoGit).unwrap_err();
    assert!(
        matches!(
            err,
            Error::Git {
                op: "for-each-ref proposal/",
                ..
            }
        ),
        "{err:?}"
    );
}

#[test]
fn an_unreadable_config_namespace_surfaces_from_the_freshness_query() {
    // `heads_at` is what "fresh" is derived from, so a git that cannot
    // enumerate the lineages must not read as "no lineage stands here",
    // which is the answer for stale.
    let (_h, ws, parent) = workspace_with_a_proposal("a1-r9");
    let err = heads_at(&ws, &parent, &NoGit).unwrap_err();
    assert!(
        matches!(
            err,
            Error::Git {
                op: "for-each-ref config/",
                ..
            }
        ),
        "{err:?}"
    );
}
