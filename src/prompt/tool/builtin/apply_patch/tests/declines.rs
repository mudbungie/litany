//! Every **typed decline** the envelope grammar gives, and the shapes
//! sitting just the other side of each: a missing marker, a line the
//! grammar cannot read, a section that changes nothing, a path opened
//! twice, an add with no body.
//!
//! Split from [`super::grammar`] at bl-517e, on the seam the file already
//! had — what parses, and what is refused. A decline's whole product is
//! its message, so these read the rendered text as well as the variant.

use super::super::parse::{Error, FileOp, parse};
use super::{envelope, parsed};

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

// bl-517e: one `*** Update File:` header per hunk is a reasonable
// misreading of a format whose hunks are separated by `@@`, and it was 2
// of 3 first-patch attempts by the shipped worker model. Both orderings
// used to decline in a voice that described a section the model does not
// believe it wrote — "has no hunks" when the second header closed an
// empty first section, "appears in more than one file operation" when it
// followed real hunks.
#[test]
fn a_second_update_header_for_one_file_names_the_cause() {
    // The verbatim envelope from the transcript: one header per hunk.
    let err = parse(&envelope(
        "*** Update File: src/cart.py\n\
         *** Update File: src/cart.py\n\
         @@\n-    return self.unit_price\n+    return self.unit_price * self.quantity\n\
         @@\n-    return round(total * (1 + rate), 2)\n+    return round(total * (1 - rate), 2)",
    ))
    .unwrap_err();
    assert_eq!(
        err,
        Error::RepeatedUpdate {
            line: 3,
            path: "src/cart.py".into()
        }
    );
    let msg = err.to_string();
    assert!(
        msg.contains("a second \"*** Update File: \" for src/cart.py"),
        "{msg}"
    );
    assert!(msg.contains("already open in this envelope"), "{msg}");
    assert!(msg.contains("separate its hunks with '@@'"), "{msg}");
    // And the other ordering — a second header after real hunks — which
    // used to reach the duplicate-path collision instead.
    let err = parse(&envelope(
        "*** Update File: a\n-x\n+y\n*** Update File: a\n-p\n+q",
    ))
    .unwrap_err();
    assert_eq!(
        err,
        Error::RepeatedUpdate {
            line: 5,
            path: "a".into()
        }
    );
}

// The refusal is about one path opened twice, not about two updates: an
// envelope updating two files reads normally.
#[test]
fn two_update_sections_for_different_files_parse() {
    let patch = parsed("*** Update File: a\n-x\n+y\n*** Update File: b\n-p\n+q");
    assert_eq!(patch.ops.len(), 2);
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
