//! The shipped **reviewer** role (`docs/DESIGN_LEARNING_LOOP.md` §2),
//! pinned the way `toolspec.rs` pins the `bash` definition: what the
//! model is *told* is the whole of what governs it, and nothing
//! downstream can correct a soul.
//!
//! The role ships **declared and unbound** (ARCH §4.3) — a row, a soul
//! and a manifest entry that cost nothing until a config binds
//! `dispatch(reviewer)`. Three things are asserted here, and each is a
//! claim the design makes rather than prose taste:
//!
//! - the soul carries hermes's **four look-fors** and its **two
//!   warnings**, because the second warning is the reason staged writes
//!   exist at all: *"the review prompt explicitly biases itself toward
//!   finding something to save"*;
//! - the grant is the **confinement** — `[apply_patch, read_file]`, no
//!   `bash`, no `dispatch`, no `message` — so a widened row fails here
//!   rather than at a live model call;
//! - the manifest entry composes the summary chain and the workspace
//!   skills, so a fresh read precedes every write by construction.

use super::*;

/// The shipped soul body with whitespace runs collapsed to one space —
/// the text that reaches the wire, read the way a claim survives a
/// reflow. A phrase is pinned for the claim it makes, not for where the
/// paragraph happened to wrap, so re-wrapping the file must not redden a
/// test and must not hide a deletion either.
fn soul() -> String {
    let raw = crate::template::TEMPLATE
        .get_file("souls/reviewer.md")
        .expect("the template ships souls/reviewer.md")
        .contents_utf8()
        .expect("souls/reviewer.md is UTF-8");
    raw.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// A claim is pinned by the phrase that makes it, so a rewording that
/// drops the claim fails here rather than silently regressing.
fn asserts(text: &str, phrases: &[&str], what: &str) {
    for phrase in phrases {
        assert!(
            text.contains(phrase),
            "the {what} no longer says {phrase:?} — it reads:\n{text}"
        );
    }
}

#[test]
fn the_reviewer_soul_carries_the_four_look_fors() {
    asserts(
        &soul(),
        &[
            "User corrections",
            "Reusable debugging or operational techniques",
            "A loaded skill that failed or is outdated",
            "A workflow worth packaging",
            "as a script, template or reference",
        ],
        "reviewer soul",
    );
}

#[test]
fn the_reviewer_soul_carries_the_two_warnings() {
    asserts(
        &soul(),
        &[
            "Do not bias toward finding something to save",
            "an empty proposal is the expected common outcome",
            "Never record an unresolved failure as a proven workflow",
        ],
        "reviewer soul",
    );
}

/// The ownership rule (§3): a pool body is the install's and is shared by
/// every workspace on the box, so a lesson learned here is a *workspace*
/// skill — and one broad skill beats a skill per incident.
#[test]
fn the_reviewer_soul_states_the_ownership_rule() {
    asserts(
        &soul(),
        &[
            "Pool skills are not yours to edit",
            "Prefer patching one broad existing workspace skill",
            "one-line subject",
        ],
        "reviewer soul",
    );
}

/// The grant is the confinement (§2): read the transcript and the
/// skills, edit files, nothing else.
#[test]
fn the_shipped_reviewer_grant_is_its_confinement() {
    let raw = crate::template::TEMPLATE
        .get_file("providers.yaml")
        .expect("the template ships providers.yaml")
        .contents_utf8()
        .expect("providers.yaml is UTF-8");
    let shipped = crate::config::PerRepoProviders::parse(raw, Path::new("template/providers.yaml"))
        .expect("the shipped template parses");

    let reviewer = &shipped.roles["reviewer"];
    let mut granted = reviewer.tools.clone();
    granted.sort();
    assert_eq!(granted, vec!["apply_patch", "read_file"]);
    // Its model is the compactor's (§2): one narrowly equipped child per
    // checkpoint, priced the same.
    assert_eq!(reviewer.model, shipped.roles["compactor"].model);
}

/// A role is its `roles:` entry **and** its soul (ARCH §4.3), so the
/// shipped template must carry both or no `dispatch(reviewer)` can ever
/// validate.
#[test]
fn the_shipped_reviewer_entry_composes_its_subject() {
    let raw = crate::template::TEMPLATE
        .get_file("manifest.yaml")
        .expect("the template ships manifest.yaml")
        .contents_utf8()
        .expect("manifest.yaml is UTF-8");
    let shipped =
        crate::config::manifest::Manifest::parse(raw, Path::new("template/manifest.yaml"))
            .expect("the shipped template parses");
    let reviewer = &shipped.roles["reviewer"];
    assert_eq!(reviewer.pinned, vec!["goal.md", "soul.md"]);
    assert_eq!(reviewer.order, vec!["summary/**", "skills/**"]);
}
