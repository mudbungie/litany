//! Grammar tests: **what parses** — every section kind, hunk separation
//! and anchoring, and the codex blank-line semantics ruled by reference
//! (bl-fdbb) — empty context inside update bodies, ignored after
//! `*** End of File`, tolerated around the envelope, declined everywhere
//! else. The typed declines are [`super::declines`]'.

use super::super::parse::{Error, FileOp, Hunk, parse};
use super::{envelope, parsed};

#[test]
fn add_delete_update_rename_in_one_envelope() {
    let patch = parsed(
        "*** Add File: new.txt\n+hello\n+world\n\
         *** Delete File: gone.txt\n\
         *** Update File: a.txt\n*** Move to: b.txt\n@@\n ctx\n-old\n+new",
    );
    assert_eq!(patch.ops.len(), 3);
    assert_eq!(
        patch.ops[0],
        FileOp::Add {
            path: "new.txt".into(),
            lines: vec!["hello".into(), "world".into()],
        }
    );
    assert_eq!(
        patch.ops[1],
        FileOp::Delete {
            path: "gone.txt".into()
        }
    );
    let FileOp::Update {
        path,
        move_to,
        hunks,
    } = &patch.ops[2]
    else {
        panic!("third op is an update");
    };
    assert_eq!(path, "a.txt");
    assert_eq!(move_to.as_deref(), Some("b.txt"));
    assert_eq!(
        hunks[..],
        [Hunk {
            anchors: vec![],
            old: vec!["ctx".into(), "old".into()],
            new: vec!["ctx".into(), "new".into()],
            eof: false,
        }]
    );
}

#[test]
fn blank_lines_around_the_envelope_are_tolerated() {
    let text = format!("\n\n{}\n\n", envelope("*** Delete File: x"));
    assert_eq!(parse(&text).unwrap().ops.len(), 1);
}

// bl-fdbb: codex declines a bare blank line between sections; it was
// silently skipped here.
#[test]
fn blank_line_between_sections_is_declined() {
    let err = parse(&envelope("*** Delete File: x\n\n*** Add File: y\n+hi")).unwrap_err();
    assert_eq!(err, Error::BlankLine { line: 3 });
    assert!(err.to_string().contains("bare blank line"), "{err}");
}

// bl-fdbb: codex declines a bare blank line in an add body; it was
// silently consumed as an empty content line (a trailing blank gave
// the added file a phantom final newline).
#[test]
fn blank_line_in_add_body_is_declined() {
    let err = parse(&envelope("*** Add File: a\n+one\n\n+three")).unwrap_err();
    assert_eq!(err, Error::BlankLine { line: 4 });
    let err = parse(&envelope("*** Add File: a\n+hello\n")).unwrap_err();
    assert_eq!(err, Error::BlankLine { line: 4 });
}

// bl-fdbb: faithful codex parity — a bare blank line anywhere in an
// update body, trailing included, is an empty context line the file
// must actually contain.
#[test]
fn blank_line_in_update_body_is_an_empty_context_line() {
    let patch = parsed("*** Update File: a\n-x\n\n+y");
    let FileOp::Update { hunks, .. } = &patch.ops[0] else {
        panic!("update");
    };
    assert_eq!(hunks[0].old, ["x", ""]);
    assert_eq!(hunks[0].new, ["", "y"]);
    let patch = parsed("*** Update File: a\n-x\n+y\n");
    let FileOp::Update { hunks, .. } = &patch.ops[0] else {
        panic!("update");
    };
    assert_eq!(hunks[0].old, ["x", ""]);
    assert_eq!(hunks[0].new, ["y", ""]);
}

// bl-fdbb: codex ignores blank lines directly after `*** End of File`
// (its own parser test carries `*** End of File\n\n*** End Patch`);
// they used to open a phantom pure-context hunk here and decline.
#[test]
fn blank_lines_after_end_of_file_are_ignored() {
    let patch = parsed("*** Update File: a\n+appended\n*** End of File\n");
    let FileOp::Update { hunks, .. } = &patch.ops[0] else {
        panic!("update");
    };
    assert_eq!(hunks.len(), 1);
    assert!(hunks[0].eof);
    assert_eq!(hunks[0].new, ["appended"]);
}

#[test]
fn bare_at_at_separates_hunks_and_anchor_after_body_opens_a_new_one() {
    let patch = parsed("*** Update File: a\n-x\n+y\n@@\n-p\n+q\n@@ fn two\n-r\n+s");
    let FileOp::Update { hunks, .. } = &patch.ops[0] else {
        panic!("update");
    };
    assert_eq!(hunks.len(), 3);
    assert!(hunks[1].anchors.is_empty());
    assert_eq!(hunks[2].anchors, ["fn two"]);
}

#[test]
fn anchors_stack_onto_one_hunk() {
    let patch = parsed("*** Update File: a\n@@ class C\n@@ fn m\n-x\n+y");
    let FileOp::Update { hunks, .. } = &patch.ops[0] else {
        panic!("update");
    };
    assert_eq!(hunks.len(), 1);
    assert_eq!(hunks[0].anchors, ["class C", "fn m"]);
}

#[test]
fn end_of_file_pins_the_hunk_and_a_trailing_bare_separator_is_ignored() {
    let patch = parsed("*** Update File: a\n+appended\n*** End of File\n@@");
    let FileOp::Update { hunks, .. } = &patch.ops[0] else {
        panic!("update");
    };
    assert_eq!(hunks.len(), 1);
    assert!(hunks[0].eof);
    assert!(hunks[0].old.is_empty());
}
