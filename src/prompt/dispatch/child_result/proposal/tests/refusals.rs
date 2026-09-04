//! Every way a proposal is refused whole (`docs/DESIGN_LEARNING_LOOP.md`
//! §3): a path outside the two classes, a lineage that moved, a
//! `SKILL.md` the authoring pass will not parse, a return that is not a
//! reviewer's, and an ending that is not a `final-response`. In each,
//! the invariant is the same one — **no ref exists** — because a
//! proposal is one commit and partial staging is a second shape.

use super::super::super::interpret_pending;
use super::super::super::tests::{Fx, returned_child, returned_child_ep, workflow};
use super::{STAGE, manifest, rev, workspace_with_a_skill};
use crate::facts;
use crate::prompt::inbox::{self, Epitaph};
use crate::workspace::fixture;
use crate::workspace::proposal::proposal_ref;

#[test]
fn an_edit_to_a_loaded_pool_copy_refuses_the_whole_proposal() {
    // §3: a body under `<data-root>/skills/` is the install's and is not
    // a reviewer's to edit — the name the pool holds is the whole test.
    let parent = "20260101-p2";
    let (_h, ws, wt) = workspace_with_a_skill(parent);
    let fx = Fx::new();
    let pool = fx.data_root().join("skills/bash");
    std::fs::create_dir_all(&pool).unwrap();
    std::fs::write(pool.join("SKILL.md"), manifest("bash", "shipped")).unwrap();
    let child = returned_child(
        &ws,
        parent,
        "reviewer",
        "review it",
        (
            "skills/bash/SKILL.md",
            "---\nname: bash\ndescription: d\n---\nmine now",
        ),
        &fx,
    );
    interpret_pending(&ws, parent, &wt, &workflow(STAGE), &fx.deps()).unwrap();
    assert_eq!(rev(&ws, &proposal_ref(&child)), None, "no ref exists");
    assert!(!wt.join("messages").exists(), "and nothing was delivered");
}

#[test]
fn an_empty_proposal_writes_no_ref_and_the_transcript_is_not_a_diff() {
    // The reviewer's only commit is a transcript entry — which every
    // branch's executor writes (ARCH §2.3) and no proposal may carry.
    // What is left is empty, so nothing is staged at all (§3 step 5).
    let parent = "20260101-p3";
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
    interpret_pending(&ws, parent, &wt, &workflow(STAGE), &fx.deps()).unwrap();
    assert_eq!(rev(&ws, &proposal_ref(&child)), None, "no ref, no notice");
}

#[test]
fn a_config_advance_between_fork_and_landing_refuses_as_stale() {
    let parent = "20260101-p4";
    let (_h, ws, wt) = workspace_with_a_skill(parent);
    let fx = Fx::new();
    let child = returned_child(
        &ws,
        parent,
        "reviewer",
        "review it",
        ("skills/notes/SKILL.md", &manifest("notes", "corrected")),
        &fx,
    );
    // The operator advances the lineage while the review is in flight.
    fixture::amend_config(&ws, &[("souls/worker.md", "a new soul")]);
    interpret_pending(&ws, parent, &wt, &workflow(STAGE), &fx.deps()).unwrap();
    assert_eq!(
        rev(&ws, &proposal_ref(&child)),
        None,
        "the review is refused whole; the next checkpoint re-derives"
    );
}

#[test]
fn a_stopped_reviewer_delivers_an_obituary_and_stages_nothing() {
    // The epitaph gate both landings share: only a `final-response`
    // return stages (§2.6/§2.7).
    let parent = "20260101-p5";
    let (_h, ws, wt) = workspace_with_a_skill(parent);
    let fx = Fx::new();
    let child = returned_child_ep(
        &ws,
        parent,
        "reviewer",
        "review it",
        ("skills/notes/SKILL.md", &manifest("notes", "half-written")),
        Epitaph::Stopped,
        &fx,
    );
    interpret_pending(&ws, parent, &wt, &workflow(STAGE), &fx.deps()).unwrap();
    assert_eq!(rev(&ws, &proposal_ref(&child)), None, "nothing staged");
    assert!(
        wt.join("messages").read_dir().unwrap().next().is_some(),
        "the obituary is delivered like any child failure"
    );
}

#[test]
fn a_return_with_no_read_mark_is_not_stageable() {
    // `stage_proposal` bound where no reviewer ever ran: a worker's
    // dispatch commit reads no config commit and marks none, so there is
    // no base to parent a proposal on. Refused, and the return is still
    // consumed.
    let parent = "20260101-p6";
    let (_h, ws, wt) = workspace_with_a_skill(parent);
    let fx = Fx::new();
    let child = returned_child(
        &ws,
        parent,
        "worker",
        "do it",
        ("out.txt", "the work\n"),
        &fx,
    );
    let w = workflow("events:\n  worker_return:\n    - stage_proposal\n");
    interpret_pending(&ws, parent, &wt, &w, &fx.deps()).unwrap();
    assert_eq!(rev(&ws, &proposal_ref(&child)), None);
    assert!(
        !inbox::inbox_dir(&ws, parent)
            .join(format!("{child}-001.md"))
            .exists(),
        "the return is consumed either way"
    );
}

#[test]
fn a_malformed_skill_is_refused_by_the_authoring_pass() {
    // Every refusal the config-authoring routine owns rides through the
    // proposal path in its own voice — a malformed `SKILL.md` here, and
    // the facts document's cap when that lands (§4).
    let parent = "20260101-p7";
    let (_h, ws, wt) = workspace_with_a_skill(parent);
    let fx = Fx::new();
    let child = returned_child(
        &ws,
        parent,
        "reviewer",
        "review it",
        (
            "skills/notes/SKILL.md",
            "---\nname: notes\ndescription: a: b\n---\nbody",
        ),
        &fx,
    );
    interpret_pending(&ws, parent, &wt, &workflow(STAGE), &fx.deps()).unwrap();
    assert_eq!(
        rev(&ws, &proposal_ref(&child)),
        None,
        "the refused pass leaves no ref behind"
    );
}

#[test]
fn an_over_cap_facts_document_is_refused_at_proposal_time() {
    // §4: the cap is the config-authoring routine's decline
    // (`crate::facts`, `docs/DESIGN_CONTEXT_ECONOMY.md` §3), and a
    // proposal is minted by that routine — so an over-cap facts document
    // is refused where it is written, never handed to an operator as a
    // proposal that cannot land.
    let parent = "20260101-p9";
    let (_h, ws, wt) = workspace_with_a_skill(parent);
    let fx = Fx::new();
    let oversized = "x".repeat(usize::try_from(facts::MAX_BYTES).unwrap() + 1);
    let child = returned_child(
        &ws,
        parent,
        "reviewer",
        "review it",
        (facts::FILE, oversized.as_str()),
        &fx,
    );
    interpret_pending(&ws, parent, &wt, &workflow(STAGE), &fx.deps()).unwrap();
    assert_eq!(
        rev(&ws, &proposal_ref(&child)),
        None,
        "the cap refuses the whole proposal, at proposal time"
    );
}
