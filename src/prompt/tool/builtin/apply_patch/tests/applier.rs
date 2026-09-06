//! Application tests: the atomic happy paths and every stale-state /
//! ambiguity decline (bl-e249). I/O-fault paths live in [`super::applier_io`].

use super::super::apply::{Error, apply};
use super::super::report::Report;
use super::parsed;
use std::fs;
use std::path::Path;
use tempfile::TempDir;

fn apply_body(body: &str, root: &Path) -> Result<Report, Error> {
    apply(&parsed(body), root)
}

fn ok(body: &str, root: &Path) -> Report {
    apply_body(body, root).expect("patch applies")
}

fn read(root: &Path, rel: &str) -> String {
    fs::read_to_string(root.join(rel)).unwrap()
}

#[test]
fn add_creates_nested_files() {
    let tmp = TempDir::new().unwrap();
    let report = ok(
        "*** Add File: d/e/new.txt\n+hello\n+world\n*** Add File: top.txt\n+solo",
        tmp.path(),
    );
    assert_eq!(read(tmp.path(), "d/e/new.txt"), "hello\nworld\n");
    assert_eq!(read(tmp.path(), "top.txt"), "solo\n");
    assert_eq!(report.status, "applied");
    assert_eq!(report.files[0].op, "add");
    assert_eq!(report.files[0].path, "d/e/new.txt");
    assert!(report.files[0].hunks.is_empty());
}

#[test]
fn delete_removes_the_file() {
    let tmp = TempDir::new().unwrap();
    fs::write(tmp.path().join("gone.txt"), "bye\n").unwrap();
    let report = ok("*** Delete File: gone.txt", tmp.path());
    assert!(!tmp.path().join("gone.txt").exists());
    assert_eq!(report.files[0].op, "delete");
}

#[test]
fn update_replaces_and_preserves_the_trailing_newline_fact() {
    let tmp = TempDir::new().unwrap();
    fs::write(tmp.path().join("a.txt"), "one\ntwo\nthree\n").unwrap();
    fs::write(tmp.path().join("b.txt"), "one\ntwo").unwrap();
    let report = ok(
        "*** Update File: a.txt\n one\n-two\n+2\n*** Update File: b.txt\n-two\n+2",
        tmp.path(),
    );
    assert_eq!(read(tmp.path(), "a.txt"), "one\n2\nthree\n");
    assert_eq!(read(tmp.path(), "b.txt"), "one\n2");
    let hunk = &report.files[0].hunks[0];
    assert_eq!((hunk.rung, hunk.line), ("exact", 1));
    assert!(
        hunk.matched.is_none(),
        "exact match reports no matched lines"
    );
}

#[test]
fn update_may_empty_the_file() {
    let tmp = TempDir::new().unwrap();
    fs::write(tmp.path().join("a.txt"), "only\n").unwrap();
    ok("*** Update File: a.txt\n-only", tmp.path());
    assert_eq!(read(tmp.path(), "a.txt"), "");
}

#[test]
fn eof_insertion_appends_including_into_an_empty_file() {
    let tmp = TempDir::new().unwrap();
    fs::write(tmp.path().join("a.txt"), "line\n").unwrap();
    fs::write(tmp.path().join("empty.txt"), "").unwrap();
    ok(
        "*** Update File: a.txt\n+tail\n*** End of File\n\
         *** Update File: empty.txt\n+first\n*** End of File",
        tmp.path(),
    );
    assert_eq!(read(tmp.path(), "a.txt"), "line\ntail\n");
    assert_eq!(read(tmp.path(), "empty.txt"), "first");
}

#[test]
fn rename_lands_after_the_hunks_and_may_create_directories() {
    let tmp = TempDir::new().unwrap();
    fs::write(tmp.path().join("a.txt"), "x\n").unwrap();
    let report = ok(
        "*** Update File: a.txt\n*** Move to: moved/b.txt\n-x\n+y",
        tmp.path(),
    );
    assert!(!tmp.path().join("a.txt").exists());
    assert_eq!(read(tmp.path(), "moved/b.txt"), "y\n");
    assert_eq!(report.files[0].moved_to.as_deref(), Some("moved/b.txt"));
}

#[test]
fn an_anchor_names_which_repeated_block_is_meant() {
    let tmp = TempDir::new().unwrap();
    let src = "fn a() {\n    ret 1\n}\nfn b() {\n    ret 1\n}\n";
    fs::write(tmp.path().join("a.rs"), src).unwrap();
    let report = ok(
        "*** Update File: a.rs\n@@ fn b() {\n-    ret 1\n+    ret 2",
        tmp.path(),
    );
    assert_eq!(
        read(tmp.path(), "a.rs"),
        "fn a() {\n    ret 1\n}\nfn b() {\n    ret 2\n}\n"
    );
    assert_eq!(report.files[0].hunks[0].line, 5);
}

#[test]
fn insertion_directly_after_an_anchor() {
    let tmp = TempDir::new().unwrap();
    fs::write(tmp.path().join("a.txt"), "head\ntail\n").unwrap();
    ok("*** Update File: a.txt\n@@ head\n+inserted", tmp.path());
    assert_eq!(read(tmp.path(), "a.txt"), "head\ninserted\ntail\n");
}

#[test]
fn later_hunks_search_only_past_earlier_ones() {
    // "dup" recurs, so naming it alone would be ambiguous — but hunk 1
    // walks past the first copy as context, and hunk 2's search starts
    // after hunk 1's replacement, where the remaining copy is unique.
    let tmp = TempDir::new().unwrap();
    fs::write(tmp.path().join("a.txt"), "dup\nx\ndup\n").unwrap();
    ok(
        "*** Update File: a.txt\n dup\n-x\n+X\n@@\n-dup\n+two",
        tmp.path(),
    );
    assert_eq!(read(tmp.path(), "a.txt"), "dup\nX\ntwo\n");
}

#[test]
fn a_fuzzy_rung_reports_the_lines_actually_replaced() {
    let tmp = TempDir::new().unwrap();
    fs::write(tmp.path().join("a.txt"), "it\u{2019}s here\n").unwrap();
    let report = ok("*** Update File: a.txt\n-it's here\n+replaced", tmp.path());
    assert_eq!(read(tmp.path(), "a.txt"), "replaced\n");
    let hunk = &report.files[0].hunks[0];
    assert_eq!(hunk.rung, "unicode-normalized");
    assert_eq!(
        hunk.matched.as_deref(),
        Some(&["it\u{2019}s here".to_string()][..])
    );
}

#[test]
fn a_decline_anywhere_applies_nothing_anywhere() {
    let tmp = TempDir::new().unwrap();
    fs::write(tmp.path().join("a.txt"), "x\n").unwrap();
    let err = apply_body(
        "*** Update File: a.txt\n-x\n+y\n*** Update File: missing.txt\n-p\n+q",
        tmp.path(),
    )
    .unwrap_err();
    assert!(matches!(err, Error::Io { action: "read", .. }), "{err}");
    // The first update was valid, but the envelope is all-or-nothing.
    assert_eq!(read(tmp.path(), "a.txt"), "x\n");
}

#[test]
fn adding_an_existing_file_is_a_stale_state_decline() {
    let tmp = TempDir::new().unwrap();
    fs::write(tmp.path().join("a.txt"), "already\n").unwrap();
    let err = apply_body("*** Add File: a.txt\n+new", tmp.path()).unwrap_err();
    assert_eq!(
        err.to_string(),
        "add a.txt: file already exists; update it or delete it first"
    );
}

#[test]
fn updating_or_deleting_a_missing_file_is_a_read_decline() {
    let tmp = TempDir::new().unwrap();
    let err = apply_body("*** Delete File: no.txt", tmp.path()).unwrap_err();
    assert!(err.to_string().starts_with("read no.txt: "), "{err}");
}

#[test]
fn a_non_utf8_target_is_declined_as_a_read_fault() {
    let tmp = TempDir::new().unwrap();
    fs::write(tmp.path().join("bin"), [0xffu8, 0xfe, 0x00]).unwrap();
    let err = apply_body("*** Update File: bin\n-x\n+y", tmp.path()).unwrap_err();
    assert!(matches!(err, Error::Io { action: "read", .. }), "{err}");
}

#[test]
fn a_rename_onto_an_existing_file_is_declined() {
    let tmp = TempDir::new().unwrap();
    fs::write(tmp.path().join("a.txt"), "x\n").unwrap();
    fs::write(tmp.path().join("b.txt"), "occupied\n").unwrap();
    let err = apply_body(
        "*** Update File: a.txt\n*** Move to: b.txt\n-x\n+y",
        tmp.path(),
    )
    .unwrap_err();
    assert_eq!(
        err.to_string(),
        "move a.txt to b.txt: destination already exists"
    );
    assert_eq!(read(tmp.path(), "b.txt"), "occupied\n");
}

#[test]
fn context_that_no_longer_exists_is_a_loud_not_found() {
    let tmp = TempDir::new().unwrap();
    fs::write(tmp.path().join("a.txt"), "drifted\n").unwrap();
    let err = apply_body("*** Update File: a.txt\n-as read\n+edit", tmp.path()).unwrap_err();
    assert_eq!(
        err.to_string(),
        "update a.txt, hunk 1: context not found at any rung (tried exact, \
         ignore-trailing-whitespace, ignore-edge-whitespace, unicode-normalized)"
    );
}

#[test]
fn ambiguous_context_is_declined_with_the_repair_recipe() {
    let tmp = TempDir::new().unwrap();
    fs::write(tmp.path().join("a.txt"), "dup\nx\ndup\n").unwrap();
    let err = apply_body("*** Update File: a.txt\n-dup\n+one", tmp.path()).unwrap_err();
    let msg = err.to_string();
    assert!(
        msg.contains("context is ambiguous — 2 matches at the exact rung"),
        "{msg}"
    );
    assert!(msg.contains("`@@ <enclosing symbol>` anchor"), "{msg}");
}

#[test]
fn a_missing_anchor_is_a_loud_not_found_naming_it() {
    let tmp = TempDir::new().unwrap();
    fs::write(tmp.path().join("a.txt"), "x\n").unwrap();
    let err = apply_body("*** Update File: a.txt\n@@ fn nope\n-x\n+y", tmp.path()).unwrap_err();
    assert!(
        err.to_string()
            .starts_with("update a.txt, hunk 1: anchor \"fn nope\" not found"),
        "{err}"
    );
}

#[test]
fn an_ambiguous_anchor_is_declined() {
    let tmp = TempDir::new().unwrap();
    fs::write(tmp.path().join("a.txt"), "fn dup\nfn dup\nx\n").unwrap();
    let err = apply_body("*** Update File: a.txt\n@@ fn dup\n-x\n+y", tmp.path()).unwrap_err();
    assert!(matches!(err, Error::Ambiguous { .. }), "{err}");
}

#[test]
fn an_insertion_with_no_location_is_declined() {
    let tmp = TempDir::new().unwrap();
    fs::write(tmp.path().join("a.txt"), "x\n").unwrap();
    let err = apply_body("*** Update File: a.txt\n+floating", tmp.path()).unwrap_err();
    assert_eq!(
        err.to_string(),
        "update a.txt, hunk 1: insertion has no location; give it context \
         lines, an `@@` anchor, or `*** End of File`"
    );
}

#[test]
fn an_absolute_patch_path_stands_as_itself() {
    let tmp = TempDir::new().unwrap();
    let abs = tmp.path().join("abs.txt");
    let other = TempDir::new().unwrap();
    ok(
        &format!("*** Add File: {}\n+via absolute", abs.display()),
        other.path(),
    );
    assert_eq!(fs::read_to_string(abs).unwrap(), "via absolute\n");
}
