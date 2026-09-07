//! The `offset`/`limit` window and the note that describes it (bl-cbe0,
//! `docs/DESIGN_CODE_EXECUTION.md` §3). Its own file because the whole
//! ladder — selection, counting, continuation, the two zero declines —
//! is one subject and [`super::tests`] is the pre-existing one.

use super::*;
use std::io::Cursor;
use tempfile::NamedTempFile;

/// Six numbered lines, each newline-terminated: `l1\nl2\n…l6\n`.
fn six_lines() -> NamedTempFile {
    let f = NamedTempFile::new().unwrap();
    std::fs::write(f.path(), b"l1\nl2\nl3\nl4\nl5\nl6\n").unwrap();
    f
}

/// Drive one invocation and hand back `(stdout, stderr)` as strings.
fn read(path: &std::path::Path, input: serde_json::Value) -> (String, String) {
    let mut object = input;
    object["path"] = serde_json::json!(path);
    let mut stdin = Cursor::new(object.to_string().into_bytes());
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();
    run(&mut stdin, &mut stdout, &mut stderr).unwrap();
    (
        String::from_utf8(stdout).unwrap(),
        String::from_utf8(stderr).unwrap(),
    )
}

#[test]
fn a_window_returns_its_lines_and_says_where_to_continue() {
    let f = six_lines();
    let (out, note) = read(f.path(), serde_json::json!({ "offset": 2, "limit": 3 }));
    assert_eq!(out, "l2\nl3\nl4\n");
    assert_eq!(
        note,
        "read_file: 3 of 6 lines from offset 2; continue with offset 5\n"
    );
}

#[test]
fn the_last_window_names_no_continuation() {
    let f = six_lines();
    let (out, note) = read(f.path(), serde_json::json!({ "offset": 5 }));
    assert_eq!(out, "l5\nl6\n");
    assert_eq!(note, "read_file: 2 of 6 lines from offset 5\n");
}

#[test]
fn a_limit_that_overruns_the_file_stops_at_the_end() {
    let f = six_lines();
    let (out, note) = read(f.path(), serde_json::json!({ "offset": 5, "limit": 999 }));
    assert_eq!(out, "l5\nl6\n");
    assert_eq!(note, "read_file: 2 of 6 lines from offset 5\n");
}

#[test]
fn a_whole_file_read_is_the_window_with_empty_inputs() {
    let f = six_lines();
    let (out, note) = read(f.path(), serde_json::json!({}));
    assert_eq!(out, "l1\nl2\nl3\nl4\nl5\nl6\n");
    assert_eq!(note, "read_file: 6 of 6 lines from offset 1\n");
}

#[test]
fn a_limit_alone_reads_from_the_first_line() {
    let f = six_lines();
    let (out, note) = read(f.path(), serde_json::json!({ "limit": 2 }));
    assert_eq!(out, "l1\nl2\n");
    assert_eq!(
        note,
        "read_file: 2 of 6 lines from offset 1; continue with offset 3\n"
    );
}

#[test]
fn an_offset_past_the_end_is_an_answer_not_a_failure() {
    // The model learns the file is shorter than it thought, in band,
    // with no `is_error` to reason about.
    let f = six_lines();
    let (out, note) = read(f.path(), serde_json::json!({ "offset": 40 }));
    assert!(out.is_empty(), "{out:?}");
    assert_eq!(note, "read_file: 0 of 6 lines from offset 40\n");
}

#[test]
fn a_final_line_without_a_newline_is_still_a_line() {
    let f = NamedTempFile::new().unwrap();
    std::fs::write(f.path(), b"a\nb").unwrap();
    let (out, note) = read(f.path(), serde_json::json!({ "offset": 2 }));
    assert_eq!(out, "b");
    assert_eq!(note, "read_file: 1 of 2 lines from offset 2\n");
}

#[test]
fn a_trailing_newline_does_not_add_an_empty_line() {
    // `wc -l` counting: three newline-terminated lines are three, not
    // four — so the total the note reports is the one the model would
    // get from the shell.
    let f = NamedTempFile::new().unwrap();
    std::fs::write(f.path(), b"a\nb\nc\n").unwrap();
    let (_, note) = read(f.path(), serde_json::json!({}));
    assert_eq!(note, "read_file: 3 of 3 lines from offset 1\n");
}

#[test]
fn a_zero_offset_is_declined_by_name() {
    let f = six_lines();
    let mut stdin = Cursor::new(
        serde_json::json!({ "path": f.path(), "offset": 0 })
            .to_string()
            .into_bytes(),
    );
    let err = run(&mut stdin, &mut Vec::new(), &mut Vec::new()).unwrap_err();
    let msg = err.to_string();
    assert!(matches!(err, Error::Range { field: "offset" }), "{msg}");
    assert!(msg.contains("lines count from 1"), "{msg}");
}

#[test]
fn a_zero_limit_is_declined_by_name() {
    let f = six_lines();
    let mut stdin = Cursor::new(
        serde_json::json!({ "path": f.path(), "limit": 0 })
            .to_string()
            .into_bytes(),
    );
    let err = run(&mut stdin, &mut Vec::new(), &mut Vec::new()).unwrap_err();
    let msg = err.to_string();
    assert!(matches!(err, Error::Range { field: "limit" }), "{msg}");
    assert!(msg.contains("limit"), "{msg}");
}

#[test]
fn a_broken_stderr_surfaces_write() {
    /// A `Write` that always errors, so the note's own write branch is
    /// exercisable. Stdout is fine here: the failure must be the
    /// note's, not the body's.
    struct BrokenWriter;
    impl Write for BrokenWriter {
        fn write(&mut self, _: &[u8]) -> io::Result<usize> {
            Err(io::Error::other("stderr pipe closed"))
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }
    let f = six_lines();
    let mut stdin = Cursor::new(
        serde_json::json!({ "path": f.path() })
            .to_string()
            .into_bytes(),
    );
    let err = run(&mut stdin, &mut Vec::new(), &mut BrokenWriter).unwrap_err();
    assert!(matches!(err, Error::Write(_)), "{err}");
}

#[test]
fn the_saturating_limit_cannot_overflow_the_last_line() {
    // `offset + limit - 1` on `u64::MAX` inputs must not wrap into a
    // window that excludes the file's own lines.
    let f = six_lines();
    let (out, note) = read(
        f.path(),
        serde_json::json!({ "offset": 1, "limit": u64::MAX }),
    );
    assert_eq!(out, "l1\nl2\nl3\nl4\nl5\nl6\n");
    assert_eq!(note, "read_file: 6 of 6 lines from offset 1\n");
}
