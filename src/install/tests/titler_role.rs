//! The shipped **titler** role (`docs/DESIGN_PROCEDURES.md` §6), pinned
//! the way [`super::planner_role`] pins the planner: what the model is
//! *told* is the whole of what governs it. Unlike the reviewer and the
//! planner it ships **declared and bound** — the conversation naming
//! itself is the ruling's named want, so every shipped workflow carries
//! its two bindings and the off switch is deleting them.

use super::*;
use crate::config::action::Action;
use crate::config::{Event, Workflow};

fn template(file: &str) -> &'static str {
    crate::template::TEMPLATE
        .get_file(file)
        .unwrap_or_else(|| panic!("the template ships {file}"))
        .contents_utf8()
        .unwrap_or_else(|| panic!("{file} is UTF-8"))
}

fn providers() -> crate::config::PerRepoProviders {
    crate::config::PerRepoProviders::parse(
        template("providers.yaml"),
        Path::new("template/providers.yaml"),
    )
    .expect("the shipped template parses")
}

/// It reads and answers; it holds no tool, so it cannot act — and it
/// thinks on the compactor's model, because a six-word title is not
/// worth a worker's call per prompt.
#[test]
fn the_titler_holds_no_tool_and_thinks_on_the_compactors_model() {
    let roles = providers().roles;
    assert!(roles["titler"].tools.is_empty());
    assert_eq!(roles["titler"].model, roles["compactor"].model);
}

/// The soul's claims, pinned by the phrases that make them.
#[test]
fn the_titler_soul_asks_for_one_line_and_nothing_else() {
    let soul = template("souls/titler.md")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    for phrase in [
        "one line of at most six words",
        "No trailing punctuation, no quotation marks, no markdown",
        "the title follows it",
        "Your terminal response **is** the title",
    ] {
        assert!(
            soul.contains(phrase),
            "the titler soul no longer says {phrase:?}"
        );
    }
}

/// The manifest entry composes the conversation as it stands: the
/// compactor's shape, for the compactor's reason.
#[test]
fn the_titler_manifest_entry_is_the_compactors_shape() {
    let shipped = crate::config::manifest::Manifest::parse(
        template("manifest.yaml"),
        Path::new("template/manifest.yaml"),
    )
    .expect("the shipped template parses");
    assert_eq!(shipped.roles["titler"], shipped.roles["compactor"]);
}

/// Declared **and bound**, in every shipped workflow: `user_message`
/// forks it and `titler_return` lands its line as `title`.
#[test]
fn every_shipped_workflow_binds_the_titler() {
    let single = std::fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("experiments/single-attempt/workflow.yaml"),
    )
    .expect("the experiment variant ships");
    for (name, raw) in [
        ("template/workflow.yaml", template("workflow.yaml")),
        ("workflows/learning-loop.yaml", LEARNING_LOOP_YAML),
        ("experiments/single-attempt/workflow.yaml", single.as_str()),
    ] {
        let workflow =
            Workflow::parse(raw, Path::new(name)).unwrap_or_else(|e| panic!("{name} parses: {e}"));
        assert_eq!(
            workflow.actions_for(&Event::UserMessage),
            vec![Action::Dispatch {
                role: "titler".into(),
                with: None,
                mode: None
            }],
            "{name}"
        );
        assert_eq!(
            workflow.actions_for(&Event::returned("titler")),
            vec![Action::LandResponse {
                file: "title".into()
            }],
            "{name}"
        );
    }
}
