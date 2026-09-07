//! The shipped **planner** role (ARCH §6 *Plan mode is a lineage*),
//! pinned the way [`super::reviewer_role`] pins the reviewer: what the
//! model is *told* is the whole of what governs it, and nothing
//! downstream can correct a soul or widen a grant back.
//!
//! The role ships **declared and unbound** (ARCH §4.3): a row, a soul
//! and a manifest entry that cost an install nothing until somebody
//! dispatches one. Three claims are asserted, and each is one the
//! design makes rather than prose taste:
//!
//! - the grant is the **confinement**, and it is the whole role — a
//!   planner that could patch, shell out, dispatch or message would be
//!   a worker with advice, so a widened row fails here rather than at a
//!   live model call;
//! - the soul says the confinement is deliberate and says what a plan
//!   is, because the failure mode of a read-only role is not acting: it
//!   is producing a page of hedging nobody can accept;
//! - nothing shipped **binds** it, which is what "declared and unbound"
//!   means and what keeps it free for an install that never uses it.

use super::*;

/// The shipped soul body with whitespace runs collapsed to one space —
/// the text that reaches the wire, read the way a claim survives a
/// reflow. A phrase is pinned for the claim it makes, never for where
/// the paragraph happened to wrap.
fn soul() -> String {
    let raw = crate::template::TEMPLATE
        .get_file("souls/planner.md")
        .expect("the template ships souls/planner.md")
        .contents_utf8()
        .expect("souls/planner.md is UTF-8");
    raw.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// The shipped `providers.yaml`, parsed.
fn providers() -> crate::config::PerRepoProviders {
    let raw = crate::template::TEMPLATE
        .get_file("providers.yaml")
        .expect("the template ships providers.yaml")
        .contents_utf8()
        .expect("providers.yaml is UTF-8");
    crate::config::PerRepoProviders::parse(raw, Path::new("template/providers.yaml"))
        .expect("the shipped template parses")
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

/// The grant is exactly the read-only set, and the exclusions are the
/// role. `bash` is the one worth being explicit about: a shell is not
/// confinable by a grant (mechanical confinement is the v1.1 sandbox's,
/// ARCH §3.6), so granting it would trade a claim that holds for one
/// that merely asks.
#[test]
fn the_planner_grant_is_its_confinement() {
    let roles = providers().roles;
    let planner = &roles["planner"];
    let mut granted = planner.tools.clone();
    granted.sort();
    assert_eq!(granted, vec!["load_skill", "read_file", "search_history"]);
    for withheld in [
        "apply_patch",
        "bash",
        "python",
        "dispatch",
        "message",
        "remember",
    ] {
        assert!(
            !planner.tools.iter().any(|t| t == withheld),
            "the planner may not call {withheld}: the grant is the whole confinement"
        );
    }
}

/// Its model is the worker's, not the compactor's: judging what a
/// change would take is the harder half of doing it, and a plan that is
/// wrong costs more than the tokens a cheaper model saves.
#[test]
fn the_planner_thinks_on_the_workers_model() {
    let roles = providers().roles;
    assert_eq!(roles["planner"].model, roles["worker"].model);
}

/// The soul states the confinement as deliberate and states what a plan
/// is. The second half is the load-bearing one: a read-only role's
/// failure mode is not acting, it is producing hedging nobody can
/// accept, so the four parts and the "do not decide" rule are what make
/// the output acceptable or rejectable at all.
#[test]
fn the_planner_soul_says_what_a_plan_is_and_why_it_cannot_act() {
    asserts(
        &soul(),
        &[
            "You hold no shell, no patch tool, no dispatch and no message",
            "What you found",
            "What you would do",
            "How it would be verified",
            "What you could not see",
            "Do not decide",
            "Do not pad",
            "Do not plan around your own confinement",
            "Your terminal response **is** the plan",
        ],
        "planner soul",
    );
}

/// Declared and **unbound**: no shipped workflow dispatches a planner,
/// which is what keeps the row free for an install that never uses it —
/// and what makes plan mode a config act (a lineage) rather than
/// something the basic agentic loop does on its own.
#[test]
fn no_shipped_workflow_binds_the_planner() {
    for (name, raw) in [
        (
            "template/workflow.yaml",
            crate::template::TEMPLATE
                .get_file("workflow.yaml")
                .expect("the template ships workflow.yaml")
                .contents_utf8()
                .expect("utf8"),
        ),
        ("workflows/learning-loop.yaml", LEARNING_LOOP_YAML),
    ] {
        let workflow = crate::config::Workflow::parse(raw, Path::new(name))
            .unwrap_or_else(|e| panic!("{name} parses: {e}"));
        for (event, actions) in workflow.typed_events() {
            for action in actions {
                assert!(
                    !matches!(&action, crate::config::action::Action::Dispatch { role, .. } if role == "planner"),
                    "{name} binds dispatch(planner) at {event:?}; the role ships unbound"
                );
            }
        }
    }
}

/// The manifest entry composes what a planner must read to plan: the
/// lineage's facts, its goal and soul, and the tool descriptions —
/// without which it would plan around tools it has — then the summary
/// chain and the workspace skills, so it does not re-propose what an
/// earlier span already settled.
#[test]
fn the_planner_manifest_entry_composes_what_it_plans_against() {
    let raw = crate::template::TEMPLATE
        .get_file("manifest.yaml")
        .expect("the template ships manifest.yaml")
        .contents_utf8()
        .expect("manifest.yaml is UTF-8");
    let shipped =
        crate::config::manifest::Manifest::parse(raw, Path::new("template/manifest.yaml"))
            .expect("the shipped template parses");
    let planner = &shipped.roles["planner"];
    assert!(planner.pinned.contains(&crate::facts::FILE.to_string()));
    assert!(planner.pinned.contains(&"descriptions/**".to_string()));
    assert_eq!(planner.order, vec!["summary/**", "skills/**"]);
}
