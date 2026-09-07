//! Unit tests for [`super::run`] (ARCH §3.3 *Paging a cut capture*).
//!
//! The subject is the whole read: an address in, a page of the calling
//! agent's own capture out, and the position line on stderr. Records
//! are written by hand — the record's shape is [`ToolOutputRecord`]'s
//! contract, pinned by the executor's own tests, so building one here
//! tests this tool rather than that one.
//!
//! Every way the read declines lives in [`declines`], split at the
//! per-file cap: the fixtures below are what both halves share.

mod declines;

use super::{Error, run};
use crate::prompt::tool::builtin::dispatch::EnvLookup;
use crate::prompt::tool::{ENV_CONV_BRANCH, ENV_CONV_REPO, ToolOutputRecord};
use std::collections::HashMap;
use std::ffi::OsString;
use std::io::{self, Cursor, Read, Write};
use std::path::{Path, PathBuf};
use tempfile::TempDir;

/// The agent every fixture calls as — a hyphenated descent (§2.3), so
/// the id in the address is not a single word by accident.
pub(super) const AGENT: &str = "root-child";

pub(super) struct StubEnv(HashMap<&'static str, OsString>);
impl EnvLookup for StubEnv {
    fn get(&self, key: &str) -> Option<OsString> {
        self.0.get(key).cloned()
    }
}

/// The two vars the harness sets on every tool subprocess (§3.3) — who
/// is calling, and where their workspace is.
pub(super) fn env(ws: &Path, agent: &str) -> StubEnv {
    let mut m = HashMap::new();
    m.insert(ENV_CONV_REPO, ws.as_os_str().to_owned());
    m.insert(ENV_CONV_BRANCH, OsString::from(agent));
    StubEnv(m)
}

/// A workspace holding one capture at
/// `steps/<AGENT>/001/tools/tu_1/output.json`.
pub(super) fn workspace(stdout: &str, stderr: &str) -> TempDir {
    let ws = TempDir::new().expect("tempdir");
    let dir = ws.path().join(record_dir());
    std::fs::create_dir_all(&dir).expect("mkdir record");
    let record = ToolOutputRecord {
        stdout: stdout.to_string(),
        stderr: stderr.to_string(),
        exit_code: 0,
        started_at: "iso-1".into(),
        ended_at: "iso-2".into(),
    };
    std::fs::write(
        dir.join("output.json"),
        serde_json::to_vec(&record).expect("record serializes"),
    )
    .expect("write record");
    ws
}

pub(super) fn record_dir() -> PathBuf {
    Path::new("steps").join(AGENT).join("001/tools/tu_1")
}

/// The address of the fixture capture's `stream` at `offset`.
pub(super) fn address(stream: &str, offset: usize) -> String {
    format!("steps/{AGENT}/001/tools/tu_1/output.json#{stream}@{offset}")
}

/// Drive the tool as the harness does, returning `(stdout, stderr)`.
pub(super) fn read(ws: &Path, agent: &str, input: &str) -> Result<(String, String), Error> {
    let mut stdin = Cursor::new(input.as_bytes().to_vec());
    let (mut stdout, mut stderr) = (Vec::new(), Vec::new());
    run(&mut stdin, &mut stdout, &mut stderr, &env(ws, agent))?;
    Ok((
        String::from_utf8(stdout).expect("ASCII fixture"),
        String::from_utf8(stderr).expect("ASCII fixture"),
    ))
}

pub(super) fn body(address: &str) -> String {
    format!("{{\"address\": \"{address}\"}}")
}

/// The core shape: the page is the stream's bytes from the offset, and
/// the position line says where the next one starts.
#[test]
fn a_page_returns_the_bytes_from_the_offset_and_names_the_next_address() {
    let stream = "x".repeat(super::PAGE_BYTES * 2);
    let ws = workspace(&stream, "");
    let (page, note) = read(ws.path(), AGENT, &body(&address("stdout", 8))).expect("reads");
    assert_eq!(page.len(), super::PAGE_BYTES);
    let total = stream.len();
    let end = 8 + super::PAGE_BYTES;
    assert_eq!(
        note,
        format!(
            "read_tool_output: bytes 8-{end} of {total} from \
             steps/{AGENT}/001/tools/tu_1/output.json stdout; continue with address {}\n",
            address("stdout", end)
        )
    );
}

/// Paging walks to the end and says so — the concatenated pages are the
/// stream's own tail from the first offset, byte for byte.
#[test]
fn paging_returns_the_remainder_and_ends() {
    let stream = "abcdefghij".repeat(1400);
    let ws = workspace(&stream, "");
    let mut at = 2048;
    let mut seen = String::new();
    let mut notes = Vec::new();
    loop {
        let (page, note) = read(ws.path(), AGENT, &body(&address("stdout", at))).expect("reads");
        seen.push_str(&page);
        at += page.len();
        notes.push(note.clone());
        if note.contains("end of stream") {
            break;
        }
        assert!(note.contains(&format!("continue with address {}", address("stdout", at))));
    }
    assert_eq!(seen, stream.get(2048..).expect("in range"));
    assert_eq!(notes.len(), 3);
    assert!(
        notes
            .last()
            .expect("a last note")
            .ends_with("; end of stream\n")
    );
}

/// The address names the stream, so `stderr` pages the other capture —
/// the two are held apart on disk exactly as the executor captured them.
#[test]
fn the_address_selects_the_stream() {
    let ws = workspace("OUT", "ERR");
    let (page, note) = read(ws.path(), AGENT, &body(&address("stderr", 0))).expect("reads");
    assert_eq!(page, "ERR");
    assert!(note.contains("bytes 0-3 of 3"), "{note}");
    assert!(note.contains("output.json stderr; end of stream"), "{note}");
}

/// An offset **at** the end is the empty final page, not a decline: the
/// general path with nothing remaining.
#[test]
fn an_offset_at_the_end_is_an_empty_final_page() {
    let ws = workspace("abc", "");
    let (page, note) = read(ws.path(), AGENT, &body(&address("stdout", 3))).expect("reads");
    assert_eq!(page, "");
    assert!(note.contains("bytes 3-3 of 3"), "{note}");
}

/// A reader that always errors, for the stdin arm.
struct FailingReader;
impl Read for FailingReader {
    fn read(&mut self, _buf: &mut [u8]) -> io::Result<usize> {
        Err(io::Error::other("stdin boom"))
    }
}

/// A writer that always errors, for the two write arms.
struct FailingWriter;
impl Write for FailingWriter {
    fn write(&mut self, _buf: &[u8]) -> io::Result<usize> {
        Err(io::Error::other("write boom"))
    }
    fn flush(&mut self) -> io::Result<()> {
        self.write(&[]).map(|_| ())
    }
}

/// A failed stdin read is its own variant, so a transient pipe failure
/// is never misattributed to the model's input.
#[test]
fn a_failed_stdin_read_is_its_own_variant() {
    let ws = workspace("abc", "");
    let err = run(
        &mut FailingReader,
        &mut Vec::new(),
        &mut Vec::new(),
        &env(ws.path(), AGENT),
    )
    .expect_err("refused");
    assert!(matches!(err, Error::StdinRead(_)));
    assert!(
        err.to_string().starts_with("read input from stdin: "),
        "{err}"
    );
}

/// Both halves of the answer are written, so a failure on either is the
/// same report: the page did not reach the model.
#[test]
fn a_failed_write_declines_on_either_stream() {
    let ws = workspace("abc", "");
    let input = body(&address("stdout", 0));
    let err = run(
        &mut Cursor::new(input.clone().into_bytes()),
        &mut FailingWriter,
        &mut Vec::new(),
        &env(ws.path(), AGENT),
    )
    .expect_err("refused");
    assert!(matches!(err, Error::Write(_)));
    assert_eq!(err.to_string(), "write the page: write boom");
    assert!(matches!(
        run(
            &mut Cursor::new(input.into_bytes()),
            &mut Vec::new(),
            &mut FailingWriter,
            &env(ws.path(), AGENT),
        )
        .expect_err("refused"),
        Error::Write(_)
    ));
    // The double's own `flush` fails the same way — exercised so it is
    // an honest writer rather than a half-implemented one.
    assert!(Write::flush(&mut FailingWriter).is_err());
}
