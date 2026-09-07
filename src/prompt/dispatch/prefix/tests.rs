//! The hold's rule table (module docs): what is held, what lands, and
//! what the record read answers when there is none.

use super::*;
use brazen::{Content, Message, Role};
use serde_json::json;

fn tool(name: &str) -> Tool {
    Tool::Custom {
        name: name.into(),
        description: Some(format!("the {name} tool")),
        input_schema: json!({"type": "object"}),
        strict: None,
    }
}

fn request(tools: &[&str], system: &str, first: &[&str]) -> CanonicalRequest {
    CanonicalRequest {
        model: "m".into(),
        system: Some(vec![Content::Text(system.into())]),
        messages: vec![Message {
            role: Role::User,
            content: first.iter().map(|t| Content::Text((*t).into())).collect(),
        }],
        tools: tools.iter().map(|n| tool(n)).collect(),
        ..CanonicalRequest::default()
    }
}

fn recorded(req: &CanonicalRequest) -> Value {
    serde_json::to_value(req).unwrap()
}

fn names(req: &CanonicalRequest) -> Vec<String> {
    req.tools
        .iter()
        .map(|t| match t {
            Tool::Custom { name, .. } => name.clone(),
            other => format!("{other:?}"),
        })
        .collect()
}

#[test]
fn a_subtraction_is_held_while_the_prefix_ahead_of_the_tail_stands() {
    let prev = recorded(&request(&["a", "b", "c"], "soul", &["head", "hi"]));
    // The tail grew by a tool round trip; `b` retired; nothing else moved.
    let mut now = request(&["a", "c"], "soul", &["head", "hi"]);
    now.messages.push(Message {
        role: Role::Assistant,
        content: vec![Content::Text("ok".into())],
    });
    let sent = hold(Some(&prev), now).unwrap();
    assert_eq!(names(&sent), ["a", "b", "c"]);
    assert_eq!(sent.messages.len(), 2, "the tail is the current one");
}

#[test]
fn an_addition_lands_and_carries_the_pending_subtraction_with_it() {
    let prev = recorded(&request(&["a", "b", "c"], "soul", &["head"]));
    let sent = hold(Some(&prev), request(&["a", "c", "d"], "soul", &["head"])).unwrap();
    assert_eq!(names(&sent), ["a", "c", "d"]);
}

#[test]
fn an_edited_definition_is_an_addition() {
    let prev = recorded(&request(&["a", "b"], "soul", &["head"]));
    let mut now = request(&["a", "b"], "soul", &["head"]);
    now.tools[1] = Tool::Custom {
        name: "b".into(),
        description: Some("b, reworded".into()),
        input_schema: json!({"type": "object"}),
        strict: None,
    };
    let sent = hold(Some(&prev), now.clone()).unwrap();
    assert_eq!(sent, now);
}

#[test]
fn a_reorder_is_not_a_subtraction() {
    let prev = recorded(&request(&["a", "b", "c"], "soul", &["head"]));
    let now = request(&["c", "a"], "soul", &["head"]);
    assert_eq!(names(&hold(Some(&prev), now).unwrap()), ["c", "a"]);
}

#[test]
fn a_system_slot_change_is_a_paid_miss_and_the_subtraction_rides_it() {
    let prev = recorded(&request(&["a", "b"], "soul", &["head"]));
    let sent = hold(Some(&prev), request(&["a"], "the edited soul", &["head"])).unwrap();
    assert_eq!(names(&sent), ["a"]);
}

#[test]
fn a_model_change_is_a_paid_miss() {
    let prev = recorded(&request(&["a", "b"], "soul", &["head"]));
    let mut now = request(&["a"], "soul", &["head"]);
    now.model = "other".into();
    assert_eq!(names(&hold(Some(&prev), now).unwrap()), ["a"]);
}

#[test]
fn a_recomposed_head_is_a_paid_miss() {
    // A compaction landing puts a summary block ahead of the tail: the
    // first wire message no longer starts with what it started with.
    let prev = recorded(&request(&["a", "b"], "soul", &["head", "hi"]));
    let sent = hold(Some(&prev), request(&["a"], "soul", &["summary", "head"])).unwrap();
    assert_eq!(names(&sent), ["a"]);
}

#[test]
fn a_first_message_that_only_grew_keeps_the_hold() {
    // A message delivered before any reply groups onto the first wire
    // message (§2.3): an append, not a recomposition.
    let prev = recorded(&request(&["a", "b"], "soul", &["head", "hi"]));
    let sent = hold(
        Some(&prev),
        request(&["a"], "soul", &["head", "hi", "and this"]),
    )
    .unwrap();
    assert_eq!(names(&sent), ["a", "b"]);
}

#[test]
fn no_record_and_no_messages_send_the_composition() {
    let now = request(&["a"], "soul", &[]);
    assert_eq!(hold(None, now.clone()).unwrap(), now);
    let mut empty = now.clone();
    empty.messages.clear();
    let prev = recorded(&empty);
    let mut held = empty.clone();
    held.tools.clear();
    // Two message-less requests: the head is intact by emptiness, and the
    // subtraction is held.
    assert_eq!(names(&hold(Some(&prev), held).unwrap()), ["a"]);
}

#[test]
fn a_held_array_that_does_not_read_back_as_tools_is_the_records_fault() {
    let mut prev = recorded(&request(&["a", "b"], "soul", &["head"]));
    prev["tools"][1] = json!({"bogus": 1});
    let err = hold(Some(&prev), request(&["a"], "soul", &["head"])).unwrap_err();
    assert!(matches!(err, Error::AdapterJson(_)), "{err}");
}

#[test]
fn the_previous_request_is_read_off_its_record_or_is_none() {
    let dir = tempfile::tempdir().unwrap();
    let ws = dir.path();
    assert!(previous(ws, "ag", 1).unwrap().is_none(), "a first step");
    assert!(
        previous(ws, "ag", 0).unwrap().is_none(),
        "no predecessor at all"
    );
    assert!(
        previous(ws, "ag", 2).unwrap().is_none(),
        "a predecessor with no request"
    );
    let step = ws.join(step_dir_rel("ag", 1));
    std::fs::create_dir_all(&step).unwrap();
    std::fs::write(step.join(REQUEST_FILE), br#"{"model":"m","tools":[]}"#).unwrap();
    assert_eq!(previous(ws, "ag", 2).unwrap().unwrap()["model"], "m");
    std::fs::write(step.join(REQUEST_FILE), b"not json").unwrap();
    assert!(matches!(previous(ws, "ag", 2), Err(Error::AdapterJson(_))));
    // An unreadable record (a directory where the file should be) is an
    // I/O fault, not "no record".
    std::fs::remove_file(step.join(REQUEST_FILE)).unwrap();
    std::fs::create_dir(step.join(REQUEST_FILE)).unwrap();
    assert!(matches!(previous(ws, "ag", 2), Err(Error::Io(_))));
}
