//! Stdio-entry tests for [`super::super::run`]: the §3.3 contract —
//! `tool_use.input` JSON on stdin, the JSON report on stdout, every
//! failure a typed [`Error`] variant. Absolute patch paths keep the
//! tests independent of the test process's own working directory.

use super::super::{Error, run};
use super::envelope;
use std::io::{self, Cursor, Read, Write};
use tempfile::TempDir;

fn run_json(input: &str) -> (Result<(), Error>, Vec<u8>) {
    let mut stdin = Cursor::new(input.as_bytes().to_vec());
    let mut stdout = Vec::new();
    let got = run(&mut stdin, &mut stdout);
    (got, stdout)
}

#[test]
fn a_patch_applies_and_the_report_lands_on_stdout() {
    let tmp = TempDir::new().unwrap();
    let path = tmp.path().join("new.txt");
    let body = format!("*** Add File: {}\n+hi", path.display());
    let input = serde_json::json!({ "input": envelope(&body) }).to_string();
    let (got, stdout) = run_json(&input);
    got.expect("patch applies");
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "hi\n");
    let report: serde_json::Value = serde_json::from_slice(&stdout).unwrap();
    assert_eq!(report["status"], "applied");
    assert_eq!(report["files"][0]["op"], "add");
    assert_eq!(report["files"][0]["path"], path.display().to_string());
}

#[test]
fn an_update_report_carries_the_rung_per_hunk() {
    let tmp = TempDir::new().unwrap();
    let path = tmp.path().join("a.txt");
    std::fs::write(&path, "x\n").unwrap();
    let body = format!("*** Update File: {}\n-x\n+y", path.display());
    let input = serde_json::json!({ "input": envelope(&body) }).to_string();
    let (got, stdout) = run_json(&input);
    got.expect("patch applies");
    let report: serde_json::Value = serde_json::from_slice(&stdout).unwrap();
    let hunk = &report["files"][0]["hunks"][0];
    assert_eq!(hunk["rung"], "exact");
    assert_eq!(hunk["line"], 1);
    assert!(hunk.get("matched").is_none(), "exact match omits `matched`");
}

#[test]
fn malformed_input_json_is_declined() {
    let (got, _) = run_json("not json");
    let err = got.unwrap_err();
    assert!(matches!(err, Error::InvalidJson(_)), "{err}");
    assert!(err.to_string().starts_with("invalid input JSON: "), "{err}");
}

#[test]
fn an_unparseable_envelope_is_a_parse_decline() {
    let input = serde_json::json!({ "input": "no markers" }).to_string();
    let (got, _) = run_json(&input);
    let err = got.unwrap_err();
    assert!(matches!(err, Error::Parse(_)), "{err}");
}

// bl-13e2: an apply decline names a path as the patch authored it, and
// the root that path resolved against — the half whose absence let a
// model read "file already exists" about a file it had just watched `rm`
// remove as a broken tool rather than as two different directories.
#[test]
fn a_refused_patch_is_an_apply_decline_naming_the_working_directory() {
    let tmp = TempDir::new().unwrap();
    let body = format!("*** Delete File: {}", tmp.path().join("absent").display());
    let input = serde_json::json!({ "input": envelope(&body) }).to_string();
    let (got, _) = run_json(&input);
    let err = got.unwrap_err();
    assert!(matches!(err, Error::Apply { .. }), "{err}");
    let cwd = std::env::current_dir().unwrap();
    assert!(
        err.to_string()
            .ends_with(&format!("(working directory: {})", cwd.display())),
        "{err}"
    );
}

// bl-13e2: the success report states it too, beside paths that are as
// authored and therefore say nothing about where they landed.
#[test]
fn the_report_names_the_directory_the_patch_acted_in() {
    let tmp = TempDir::new().unwrap();
    let body = format!(
        "*** Add File: {}\n+hi",
        tmp.path().join("new.txt").display()
    );
    let input = serde_json::json!({ "input": envelope(&body) }).to_string();
    let (got, stdout) = run_json(&input);
    got.expect("patch applies");
    let report: serde_json::Value = serde_json::from_slice(&stdout).unwrap();
    assert_eq!(
        report["root"],
        std::env::current_dir().unwrap().display().to_string()
    );
}

/// A reader whose first read fails: the harness stdin-pipe fault.
struct FailingReader;
impl Read for FailingReader {
    fn read(&mut self, _buf: &mut [u8]) -> io::Result<usize> {
        Err(io::Error::other("pipe broke"))
    }
}

#[test]
fn a_failing_stdin_pipe_is_a_stdin_fault() {
    let mut stdout = Vec::new();
    let err = run(&mut FailingReader, &mut stdout).unwrap_err();
    assert_eq!(err.to_string(), "read input from stdin: pipe broke");
}

/// A writer that always fails: the harness stdout-pipe fault.
struct FailingWriter;
impl Write for FailingWriter {
    fn write(&mut self, _buf: &[u8]) -> io::Result<usize> {
        Err(io::Error::other("stdout gone"))
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

#[test]
fn a_failing_stdout_pipe_is_a_stdout_fault_after_the_patch_applied() {
    let tmp = TempDir::new().unwrap();
    let path = tmp.path().join("new.txt");
    let body = format!("*** Add File: {}\n+hi", path.display());
    let input = serde_json::json!({ "input": envelope(&body) }).to_string();
    let mut stdin = Cursor::new(input.into_bytes());
    let err = run(&mut stdin, &mut FailingWriter).unwrap_err();
    assert_eq!(err.to_string(), "write to stdout: stdout gone");
    // The side effect had already landed; only the report was lost.
    assert!(path.exists());
    // The stub's no-op flush is part of its Write contract.
    FailingWriter.flush().unwrap();
}
