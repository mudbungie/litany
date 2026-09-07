//! The **role** half of the retarget landing (ARCH §4.3, §6 *Plan mode
//! is a lineage*, bl-946c): the mark that settles an agent on a named
//! role, landed against a real workspace here and against [`super::stub`]'s
//! scripted git for the arms a real one cannot reach. Split from
//! [`super`] and from [`super::stub`] at the per-file cap, along the
//! seam the feature itself draws — the config half of the same landing
//! is what stays in each.

use super::stub::Script;
use super::*;

/// Mark a **role** and land it, with no config mark beside it — the
/// role-only retarget (§4.3, bl-946c).
fn settle_role(ws: &Path, wt: &Path, role: &str) -> Option<Outcome> {
    crate::workspace::retarget::write_role(ws, "a", role, &g()).unwrap();
    land(ws, "a", wt, &g()).unwrap()
}

#[test]
fn a_role_mark_alone_settles_the_branch_on_that_role_from_the_same_config() {
    // THE ROLE PIN (§4.3, §6 *Plan mode*, bl-946c): no lineage moves, no
    // `litany config` pass happened, and the branch is a planner from
    // here on — its soul re-pinned from the same commit, its grant cut
    // to the planner row, and the fact written where every later step
    // reads it (the dispatch commit's subject).
    let (_h, ws, wt) = agent();
    let governing_before = governing(&ws, &agent_ref("a"), &g()).unwrap();
    step(
        &wt,
        "messages/001-user.md",
        "how would you add X?\n",
        "transcript 001: user [a]",
    );

    assert_eq!(settle_role(&ws, &wt, "planner"), Some(Outcome::Landed));

    assert_eq!(
        governing(&ws, &agent_ref("a"), &g()).unwrap(),
        governing_before,
        "the role settles from the same config commit",
    );
    assert_eq!(
        subjects(&wt)[..2],
        ["transcript 001: user [a]", "dispatch: planner [a]"],
    );
    let soul = std::fs::read_to_string(wt.join("soul.md")).unwrap();
    assert!(soul.starts_with("# Planner"), "got {soul:?}");
    assert!(wt.join("descriptions/tools/read_file.json").exists());
    assert!(
        !wt.join("descriptions/tools/bash.json").exists(),
        "the grant is the confinement: a planner holds no shell",
    );
    // Both marks are answered, whichever stood (§2.2).
    assert!(crate::workspace::retarget::read_role(&ws, "a", &g()).is_none());
}

#[test]
fn the_role_a_landing_settles_is_what_the_next_resolution_reads() {
    // The role's one home is the dispatch subject, so the settled role
    // is not a second fact this landing also has to record.
    let (_h, ws, wt) = agent();
    settle_role(&ws, &wt, "planner");
    assert_eq!(
        role::derive(&wt, &agent_ref("a"), "a", &g())
            .unwrap()
            .as_deref(),
        Some("planner"),
    );
}

#[test]
fn a_role_mark_alone_lands_without_the_lineage_moving() {
    // The role half (bl-946c): `rev-parse` on the retarget mark fails,
    // so only the role mark stands — and the landing re-forks onto the
    // commit already governing, under the marked role.
    let s = Script {
        fail_capture: Some("--verify refs/litany/retarget"),
        role_mark: "planner",
        ..Script::default()
    };
    assert_eq!(s.land().unwrap(), Some(Outcome::Landed));
}

#[test]
fn an_unreadable_role_mark_leaves_the_branch_its_committed_role() {
    // A role mark that will not read is no mark at all, exactly as an
    // unreadable retarget mark is — the config mark alone then lands.
    let s = Script {
        fail_capture: Some("cat-file"),
        ..Script::default()
    };
    assert_eq!(s.land().unwrap(), Some(Outcome::Landed));
}
