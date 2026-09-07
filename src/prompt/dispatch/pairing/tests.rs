//! Unit arms for the pairing invariant (ARCH §2.3, §2.5; bl-2d93).

use super::*;
use brazen::Role;

fn use_block(id: &str) -> Content {
    Content::ToolUse {
        id: id.to_string(),
        name: "bash".to_string(),
        input: serde_json::json!({}),
        signature: None,
    }
}

fn result_block(id: &str) -> Content {
    Content::ToolResult {
        tool_use_id: id.to_string(),
        content: vec![Content::Text("ok".to_string())],
        is_error: false,
    }
}

fn message(role: Role, content: Vec<Content>) -> Message {
    Message { role, content }
}

#[test]
fn order_lives_in_the_counter_not_in_the_string() {
    // Past 999 the zero pad stops making lexical and numeric order
    // agree, and a name with no counter is no entry.
    let ordered = ordered(vec![
        "messages/1000-tool.json".to_string(),
        "messages/999-m.json".to_string(),
        "messages/README".to_string(),
    ]);
    assert_eq!(
        ordered,
        vec![
            "messages/999-m.json".to_string(),
            "messages/1000-tool.json".to_string()
        ]
    );
}

#[test]
fn a_transcript_with_no_model_entry_has_no_unsettled_window() {
    let entries = vec!["messages/001-user.md".to_string()];
    let never = |_: &str| -> Result<Vec<Content>, Error> { unreachable!("no json entry is read") };
    assert!(unsettled_from(&entries, &never).unwrap().is_none());
}

#[test]
fn an_answered_window_is_settled_and_an_unanswered_one_cuts_at_the_model_entry() {
    let entries = vec![
        "messages/001-user.md".to_string(),
        "messages/002-m.json".to_string(),
        "messages/003-tool.json".to_string(),
    ];
    let answered = |rel: &str| -> Result<Vec<Content>, Error> {
        Ok(match kind(rel) {
            Kind::Model => vec![use_block("c1")],
            _ => vec![result_block("c1")],
        })
    };
    assert!(unsettled_from(&entries, &answered).unwrap().is_none());

    // The same window with the result entry not yet committed: the cut
    // is the model entry's index, and everything from it is one unit.
    let open = |_: &str| -> Result<Vec<Content>, Error> { Ok(vec![use_block("c1")]) };
    assert_eq!(unsettled_from(&entries[..2], &open).unwrap(), Some(1));
}

#[test]
fn a_read_failure_surfaces_rather_than_reading_as_settled() {
    let entries = vec!["messages/002-m.json".to_string()];
    let broken = |_: &str| -> Result<Vec<Content>, Error> {
        Err(Error::Io(std::io::Error::other("blob gone")))
    };
    assert!(matches!(
        unsettled_from(&entries, &broken),
        Err(Error::Io(_))
    ));
}

#[test]
fn a_read_failure_on_an_answering_entry_surfaces_too() {
    let entries = vec![
        "messages/002-m.json".to_string(),
        "messages/003-tool.json".to_string(),
    ];
    let broken = |rel: &str| -> Result<Vec<Content>, Error> {
        match kind(rel) {
            Kind::Model => Ok(vec![use_block("c1")]),
            _ => Err(Error::Io(std::io::Error::other("blob gone"))),
        }
    };
    assert!(matches!(
        unsettled_from(&entries, &broken),
        Err(Error::Io(_))
    ));
}

#[test]
fn an_orphan_result_is_dropped_and_named_while_the_paired_one_stays() {
    let mut messages = vec![
        message(Role::User, vec![Content::Text("go".into())]),
        message(Role::Assistant, vec![use_block("c1")]),
        message(Role::Tool, vec![result_block("c1"), result_block("gone")]),
    ];
    assert_eq!(drop_orphans(&mut messages), vec!["gone".to_string()]);
    assert_eq!(messages.len(), 3);
    assert_eq!(messages[2].content, vec![result_block("c1")]);
}

#[test]
fn a_message_left_empty_by_the_drop_goes_with_its_last_block() {
    // The wedged shape from the field: the call's entry was swept out of
    // context and the result's entry survived, so the whole tool message
    // is an orphan and no lawful wire history carries it.
    let mut messages = vec![
        message(Role::User, vec![Content::Text("go".into())]),
        message(Role::Tool, vec![result_block("call_x")]),
    ];
    assert_eq!(drop_orphans(&mut messages), vec!["call_x".to_string()]);
    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0].role, Role::User);
}

#[test]
fn a_history_with_no_orphan_is_untouched() {
    let mut messages = vec![
        message(Role::Assistant, vec![use_block("c1")]),
        message(Role::Tool, vec![result_block("c1")]),
    ];
    let before = messages.clone();
    assert!(drop_orphans(&mut messages).is_empty());
    assert_eq!(messages, before);
}
