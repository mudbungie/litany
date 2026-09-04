//! A reviewer's **staged proposal is reaped with it** (ARCH §9.2,
//! `docs/DESIGN_LEARNING_LOOP.md` §3): `proposal/<reviewer-id>` is one
//! more home an agent id has, so `litany delete` takes it exactly as it
//! takes the agent's branch and its marks. Split from [`super::delete`]
//! only for the per-file cap; the fixture is that file's, in miniature.

use super::super::delete;
use crate::template::{GitRunner, RealGit};
use crate::workspace::{self, agent_ref, config_ref, proposal::proposal_ref, repo_git};

const REVIEWER: &str = "20260101-a1-20260102-r2";

#[test]
fn deleting_a_reviewer_reaps_its_proposal_and_its_read_mark() {
    let (_h, ws) = workspace::fixture::workspace();
    workspace::fixture::spawn_root(&ws, "20260101-a1");
    workspace::fixture::spawn_agent(&ws, REVIEWER, &agent_ref("20260101-a1"));
    let git = RealGit::new();
    let repo = repo_git(&ws);
    let tip = git
        .run_capture(&repo, &["rev-parse", &config_ref("default")])
        .unwrap();
    let tip = tip.trim();
    // The two refs a reviewer leaves behind: what it read, and what it
    // proposed.
    workspace::proposal::write_read_mark(&repo, REVIEWER, tip, &git).unwrap();
    git.run(&repo, &["branch", &proposal_ref(REVIEWER), tip])
        .unwrap();

    delete(&ws, REVIEWER, false, false, &git).unwrap();

    let refs = git
        .run_capture(&repo, &["for-each-ref", "--format=%(refname)"])
        .unwrap();
    assert!(
        !refs.contains(REVIEWER),
        "no ref of any kind still names the reviewer: {refs}"
    );
    assert!(
        refs.contains(&config_ref("default")),
        "and the lineage it read is untouched: {refs}"
    );
}

#[test]
fn deleting_a_reviewer_that_proposed_nothing_is_the_same_act() {
    // The absent proposal ref is the general path with empty inputs: the
    // delete names it unconditionally and `update-ref -d` is already the
    // postcondition, so an agent that never proposed needs no arm.
    let (_h, ws) = workspace::fixture::workspace();
    workspace::fixture::spawn_root(&ws, "20260101-a1");
    let report = delete(&ws, "20260101-a1", false, false, &RealGit::new()).unwrap();
    assert!(report.removed);
    assert!(!workspace::agent_exists(
        &ws,
        "20260101-a1",
        &RealGit::new()
    ));
}
