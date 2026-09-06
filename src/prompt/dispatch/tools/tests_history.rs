//! The **history closure** at the composer ([`super::history`], ARCH
//! §3.3): what the request declares for a tool the assembled transcript
//! names and the role's own toolset does not carry. Split from
//! [`super::tests`], which owns election, to hold the per-file line cap.
//!
//! Both directions are pinned here, because the closure now asks the
//! grant gate rather than guessing: a name the role may not call is
//! declared as refused (bl-9c1d), and a name it may call keeps its
//! definition.

use super::tests::{BASH_SCHEMA, custom, granted, history_calling, write_schema, write_skill};
use super::*;
use brazen::Content;
use serde_json::json;
use tempfile::TempDir;

#[test]
fn a_tool_the_history_names_but_the_role_may_not_call_is_declared_as_refused() {
    // THE PIN (bl-9c1d): the compactor case (§2.7) — the inherited
    // transcript calls `bash`, which the role's grant does not carry, so
    // the name must be declared or the provider refuses the history. It
    // is declared with the DOOR'S OWN SENTENCE as its description and an
    // opaque schema, not with the real definition: sent as an offer, it
    // was taken as one — 360 refused `bash` calls across 33 reviewer
    // branches of one lane, each a full model round trip over the whole
    // inherited transcript. The committed description and schema are
    // present here and deliberately unused.
    let wt = TempDir::new().unwrap();
    write_schema(wt.path(), "bash", BASH_SCHEMA);
    write_skill(
        wt.path(),
        "bash",
        "name: bash\ndescription: Run a shell command.\n",
    );

    let tools = compose(wt.path(), &granted(&[]), &history_calling(&["bash"]), &[]).unwrap();

    assert_eq!(tools.len(), 1);
    let (name, description, input_schema) = custom(&tools[0]);
    assert_eq!(name, "bash");
    let description = description.expect("a refused entry says so");
    assert!(
        description.contains("not callable by a worker"),
        "{description}"
    );
    assert!(
        description.contains("declaring is not permitting"),
        "{description}"
    );
    assert_eq!(*input_schema, json!({"type":"object"}));
    // It is the door's sentence, not a second copy of it.
    assert_eq!(
        Some(description.to_string()),
        crate::prompt::dispatch::tool_step::permit::refusal("worker", &[], &[], "bash")
    );
}

#[test]
fn a_granted_tool_the_history_names_keeps_its_definition() {
    // The other direction: `refusal` answers `None` for a name the role
    // may call, so the closure does not mislabel it. A granted name with
    // a committed schema is elected before the closure ever sees it; one
    // the tree describes no schema for reaches the closure and composes
    // with the stand-in schema and its skill description — undescribed is
    // not revoked (§3.3).
    let wt = TempDir::new().unwrap();
    write_skill(
        wt.path(),
        "bash",
        "name: bash\ndescription: Run a shell command.\n",
    );
    let grant = ["bash".to_string()];
    let tools = compose(
        wt.path(),
        &granted(&grant),
        &history_calling(&["bash"]),
        &[],
    )
    .unwrap();

    assert_eq!(tools.len(), 1);
    let (name, description, input_schema) = custom(&tools[0]);
    assert_eq!(name, "bash");
    assert_eq!(description, Some("Run a shell command."));
    assert_eq!(*input_schema, json!({"type":"object"}));
}

#[test]
fn an_already_declared_tool_is_never_declared_twice() {
    let wt = TempDir::new().unwrap();
    write_schema(wt.path(), "bash", BASH_SCHEMA);
    // Named twice by the history, and already elected: still one entry.
    let declared = ["bash".to_string()];
    let tools = compose(
        wt.path(),
        &granted(&declared),
        &history_calling(&["bash", "bash"]),
        &[],
    )
    .unwrap();
    let names: Vec<&str> = tools.iter().map(|t| custom(t).0).collect();
    assert_eq!(names, vec!["bash"]);
}

#[test]
fn a_history_with_no_tool_use_leaves_the_declaration_alone() {
    // The plain no-tools path: nothing to close over, nothing appended.
    let wt = TempDir::new().unwrap();
    write_schema(wt.path(), "bash", BASH_SCHEMA);
    let history = vec![Message {
        role: brazen::Role::User,
        content: vec![Content::Text("just talking".into())],
    }];
    let declared = ["bash".to_string()];
    let tools = compose(wt.path(), &granted(&declared), &history, &[]).unwrap();
    let names: Vec<&str> = tools.iter().map(|t| custom(t).0).collect();
    assert_eq!(names, vec!["bash"]);
}
