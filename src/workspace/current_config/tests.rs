//! The followed-config derivation against a real workspace (ARCH §2.2
//! *Fork chooses the lineage*, bl-403b).

use super::{Resolution, current_config};
use crate::template::{GitRunner, RealGit};
use crate::workspace::{config_ref, fixture, repo_git};
use std::path::Path;

fn head(ws: &Path, lineage: &str) -> String {
    RealGit::new()
        .run_capture(&repo_git(ws), &["rev-parse", &config_ref(lineage)])
        .unwrap()
        .trim()
        .to_string()
}

#[test]
fn an_unadvanced_lineage_answers_its_own_head_which_is_the_fork_commit() {
    // The degenerate follow: tip == governing. One rule, no fresh-start
    // special case.
    let (_h, ws) = fixture::workspace();
    fixture::spawn_root(&ws, "20260101-a1");
    let got = current_config(&ws, "agents/20260101-a1", &RealGit::new()).unwrap();
    assert_eq!(got, Resolution::Tip(head(&ws, "default")));
}

#[test]
fn an_advanced_lineage_is_followed_to_its_current_tip() {
    // THE RULING (2026-09-01): the conversation is not pinned — a config
    // edit after the fork reaches the running agent at its next step.
    let (_h, ws) = fixture::workspace();
    fixture::spawn_root(&ws, "20260101-a1");
    let old = head(&ws, "default");
    fixture::amend_config(&ws, &[("souls/worker.md", "a newer soul\n")]);
    let new = head(&ws, "default");
    assert_ne!(old, new);
    let got = current_config(&ws, "agents/20260101-a1", &RealGit::new()).unwrap();
    assert_eq!(got, Resolution::Tip(new));
}

#[test]
fn undiverged_sibling_lineages_still_follow_the_one_tip_they_share() {
    // A variant lineage forked at the head and not yet advanced: two
    // refs, one distinct tip — followed, with nothing to guess between.
    let (_h, ws) = fixture::workspace();
    fixture::spawn_root(&ws, "20260101-a1");
    let git = RealGit::new();
    let tip = head(&ws, "default");
    git.run(
        &repo_git(&ws),
        &["update-ref", "refs/heads/config/variant", &tip],
    )
    .unwrap();
    let got = current_config(&ws, "agents/20260101-a1", &git).unwrap();
    assert_eq!(got, Resolution::Tip(tip));
}

#[test]
fn diverged_lineages_hold_the_agent_on_its_fork_commit() {
    // Two lineages through the same fork, then one advances: real
    // divergence. The derivation must not guess, so the agent resolves
    // its governing commit — the pre-ruling answer — and `litany
    // retarget` settles the lineage.
    let (_h, ws) = fixture::workspace();
    fixture::spawn_root(&ws, "20260101-a1");
    let git = RealGit::new();
    let fork = head(&ws, "default");
    git.run(
        &repo_git(&ws),
        &["update-ref", "refs/heads/config/variant", &fork],
    )
    .unwrap();
    fixture::amend_config(&ws, &[("souls/worker.md", "a newer soul\n")]);
    let got = current_config(&ws, "agents/20260101-a1", &git).unwrap();
    assert_eq!(
        got,
        Resolution::ForkCommit {
            commit: fork,
            tips: 2
        }
    );
    assert_eq!(got.held(), Some(2));
}

#[test]
fn commit_answers_both_arms() {
    assert_eq!(Resolution::Tip("t".into()).commit(), "t");
    assert_eq!(
        Resolution::ForkCommit {
            commit: "c".into(),
            tips: 2
        }
        .commit(),
        "c"
    );
    assert_eq!(Resolution::Tip("t".into()).held(), None);
}
