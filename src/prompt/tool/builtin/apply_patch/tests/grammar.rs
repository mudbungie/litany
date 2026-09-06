//! Grammar tests: every section kind, every decline, and the codex
//! blank-line semantics ruled by reference (bl-fdbb) — empty context
//! inside update bodies, ignored after `*** End of File`, tolerated
//! around the envelope, declined everywhere else.

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

#[test]
fn missing_begin_is_declined_for_empty_and_mismarked_input() {
    assert_eq!(parse(""), Err(Error::MissingBegin));
    assert_eq!(parse("   \n \n"), Err(Error::MissingBegin));
    assert_eq!(parse("hello\n*** End Patch"), Err(Error::MissingBegin));
    let err = parse("nope").unwrap_err();
    assert_eq!(err.to_string(), "patch must start with \"*** Begin Patch\"");
}

#[test]
fn missing_end_is_declined() {
    let err = parse("*** Begin Patch\n*** Delete File: x").unwrap_err();
    assert_eq!(err, Error::MissingEnd);
    assert_eq!(err.to_string(), "patch must end with \"*** End Patch\"");
}

#[test]
fn an_envelope_with_no_operations_is_declined() {
    let err = parse("*** Begin Patch\n*** End Patch").unwrap_err();
    assert_eq!(err.to_string(), "patch contains no file operations");
}

#[test]
fn garbage_between_sections_is_a_bad_line_naming_it() {
    let err = parse(&envelope("*** Delete File: x\nwhat is this")).unwrap_err();
    assert_eq!(
        err,
        Error::BadLine {
            line: 3,
            content: "what is this".into()
        }
    );
    assert!(err.to_string().contains("unrecognized patch line"), "{err}");
}

#[test]
fn garbage_inside_an_update_body_is_a_bad_line() {
    let err = parse(&envelope("*** Update File: a\n-x\n+y\n?stray")).unwrap_err();
    assert_eq!(
        err,
        Error::BadLine {
            line: 5,
            content: "?stray".into()
        }
    );
}

#[test]
fn move_to_outside_an_update_is_declined() {
    let err = parse(&envelope("*** Move to: b")).unwrap_err();
    assert_eq!(err, Error::MisplacedMove { line: 2 });
    assert!(err.to_string().contains("must directly follow"), "{err}");
}

#[test]
fn an_update_with_no_hunks_is_declined() {
    let err = parse(&envelope("*** Update File: a")).unwrap_err();
    assert_eq!(err, Error::EmptyUpdate { path: "a".into() });
    assert_eq!(err.to_string(), "update of a has no hunks");
}

#[test]
fn a_pure_context_hunk_changes_nothing_and_is_declined() {
    let err = parse(&envelope("*** Update File: a\n ctx only")).unwrap_err();
    assert_eq!(
        err,
        Error::NoChange {
            path: "a".into(),
            hunk: 1
        }
    );
    assert_eq!(err.to_string(), "update of a: hunk 1 changes nothing");
}

#[test]
fn a_lone_end_of_file_marker_changes_nothing_and_is_declined() {
    let err = parse(&envelope("*** Update File: a\n*** End of File")).unwrap_err();
    assert_eq!(
        err,
        Error::NoChange {
            path: "a".into(),
            hunk: 1
        }
    );
}

#[test]
fn a_path_named_twice_is_declined() {
    let err = parse(&envelope("*** Delete File: x\n*** Update File: x\n-a\n+b")).unwrap_err();
    assert_eq!(err, Error::DuplicatePath { path: "x".into() });
    assert_eq!(err.to_string(), "x appears in more than one file operation");
}

#[test]
fn a_rename_target_colliding_with_another_operation_is_declined() {
    let err = parse(&envelope(
        "*** Add File: b\n+hi\n*** Update File: a\n*** Move to: b\n-x\n+y",
    ))
    .unwrap_err();
    assert_eq!(err, Error::DuplicatePath { path: "b".into() });
}

#[test]
fn an_add_section_may_close_the_envelope() {
    let patch = parsed("*** Add File: one.txt\n+hello");
    assert_eq!(
        patch.ops[0],
        FileOp::Add {
            path: "one.txt".into(),
            lines: vec!["hello".into()],
        }
    );
}

// bl-c4a2: an add with no `+` lines used to parse, write a 0-byte file
// and answer `applied` — a receipt that stops the model retrying, on a
// name now locked against the retry that would have written it.
#[test]
fn an_add_section_with_no_body_lines_is_declined() {
    for body in [
        "*** Add File: empty.txt",
        "*** Add File: empty.txt\n*** Delete File: gone.txt",
    ] {
        let err = parse(&envelope(body)).unwrap_err();
        assert_eq!(
            err,
            Error::EmptyAdd {
                path: "empty.txt".into()
            }
        );
        assert!(
            err.to_string()
                .contains("add of empty.txt has no content lines"),
            "{err}"
        );
        // The message names the repair, which is the whole point of the
        // refusal: the model reads this text and re-authors the section.
        assert!(err.to_string().contains("with a leading '+'"), "{err}");
    }
}
