//! Unit tests for [`super::run`]. Each branch of the function plus
//! every error variant lands in its own test so a coverage regression
//! points at the offending path.

use super::*;
use std::io::Cursor;
use tempfile::{NamedTempFile, TempDir};

fn input_for(path: &std::path::Path) -> Vec<u8> {
    serde_json::json!({ "path": path }).to_string().into_bytes()
}

/// A temp file whose `stat` size is exactly `size`, allocated sparsely
/// (seek past the end, write one byte) so an over-cap fixture costs no
/// real disk. Returns the holder — dropping it removes the file.
fn sparse_file(size: u64) -> NamedTempFile {
    use std::io::{Seek, SeekFrom, Write};
    let f = NamedTempFile::new().unwrap();
    let mut handle = std::fs::OpenOptions::new()
        .write(true)
        .open(f.path())
        .unwrap();
    handle.seek(SeekFrom::Start(size - 1)).unwrap();
    handle.write_all(&[0u8]).unwrap();
    handle.sync_all().unwrap();
    f
}

#[test]
fn happy_path_reads_file_to_stdout() {
    let f = NamedTempFile::new().unwrap();
    std::fs::write(f.path(), b"hello bytes").unwrap();
    let mut stdin = Cursor::new(input_for(f.path()));
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();
    run(&mut stdin, &mut stdout, &mut stderr).unwrap();
    assert_eq!(stdout, b"hello bytes");
    // Stdout is the file verbatim — the counts ride stderr, which the
    // envelope surfaces on success too (ARCH §3.3).
    assert_eq!(
        String::from_utf8(stderr).unwrap(),
        "read_file: 1 of 1 lines from offset 1\n"
    );
}

#[test]
fn empty_file_yields_empty_stdout() {
    let f = NamedTempFile::new().unwrap();
    let mut stdin = Cursor::new(input_for(f.path()));
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();
    run(&mut stdin, &mut stdout, &mut stderr).unwrap();
    assert!(stdout.is_empty());
    // An empty file has no lines, and the note says so rather than
    // needing an arm of its own.
    assert_eq!(
        String::from_utf8(stderr).unwrap(),
        "read_file: 0 of 0 lines from offset 1\n"
    );
}

#[test]
fn invalid_json_input_surfaces_invalid_json() {
    let mut stdin = Cursor::new(b"not json".to_vec());
    let mut stdout = Vec::new();
    let err = run(&mut stdin, &mut stdout, &mut Vec::new()).unwrap_err();
    assert!(matches!(err, Error::InvalidJson(_)), "{err}");
}

#[test]
fn missing_path_field_surfaces_invalid_json() {
    let mut stdin = Cursor::new(br#"{"other": "field"}"#.to_vec());
    let mut stdout = Vec::new();
    let err = run(&mut stdin, &mut stdout, &mut Vec::new()).unwrap_err();
    assert!(matches!(err, Error::InvalidJson(_)), "{err}");
}

#[test]
fn extra_fields_rejected_by_deny_unknown_fields() {
    // `serde(deny_unknown_fields)` — a typo'd field is not silently
    // ignored, so the model sees an explicit failure.
    let mut stdin = Cursor::new(br#"{"path": "/tmp/x", "extra": 1}"#.to_vec());
    let mut stdout = Vec::new();
    let err = run(&mut stdin, &mut stdout, &mut Vec::new()).unwrap_err();
    assert!(matches!(err, Error::InvalidJson(_)), "{err}");
}

#[test]
fn missing_file_surfaces_open_error() {
    let dir = TempDir::new().unwrap();
    let nope = dir.path().join("does-not-exist");
    let mut stdin = Cursor::new(input_for(&nope));
    let mut stdout = Vec::new();
    let err = run(&mut stdin, &mut stdout, &mut Vec::new()).unwrap_err();
    let msg = err.to_string();
    assert!(matches!(err, Error::Open { .. }), "{msg}");
    assert!(msg.contains("does-not-exist"), "{msg}");
}

#[test]
fn directory_path_surfaces_read_or_open_error() {
    // `File::open(dir)` succeeds on Linux (the kernel hands back a
    // directory fd), and the subsequent `read_to_end` returns EISDIR,
    // landing on `Error::Read`. On platforms where `File::open`
    // rejects the directory directly, `Error::Open` is the surface.
    // Both are tool-level failures rather than panics, and exercising
    // either lights up the `Error::Read` (or `Error::Open`) branch.
    let dir = TempDir::new().unwrap();
    let mut stdin = Cursor::new(input_for(dir.path()));
    let mut stdout = Vec::new();
    let err = run(&mut stdin, &mut stdout, &mut Vec::new()).unwrap_err();
    assert!(
        matches!(err, Error::Open { .. } | Error::Read { .. }),
        "{err}",
    );
}

#[test]
fn oversized_file_surfaces_too_large() {
    // Construct a file well over the cap and confirm the hard-reject
    // path. Use seek-and-write so we don't actually allocate the bytes.
    // The size is deliberately far above `cap + 1` — the capped read's
    // own length — so the reported size can only come from `stat`.
    let true_size = MAX_BYTES * 7 + 3;
    let f = sparse_file(true_size);

    let mut stdin = Cursor::new(input_for(f.path()));
    let mut stdout = Vec::new();
    let err = run(&mut stdin, &mut stdout, &mut Vec::new()).unwrap_err();
    let msg = err.to_string();
    assert!(
        matches!(err, Error::TooLarge { size, cap, .. } if size == true_size && cap == MAX_BYTES),
        "{msg}",
    );
    assert!(msg.contains(&format!("is {true_size} bytes")), "{msg}");
    assert!(msg.contains(&format!("cap {MAX_BYTES}")), "{msg}");
}

#[test]
fn oversize_sizes_are_the_files_own_not_the_capped_read_length() {
    // Two different oversize files must report two different sizes:
    // the pre-fix message fabricated `cap + 1` for every one of them.
    let msg_for = |size: u64| {
        let f = sparse_file(size);
        let mut stdin = Cursor::new(input_for(f.path()));
        let mut stdout = Vec::new();
        run(&mut stdin, &mut stdout, &mut Vec::new())
            .unwrap_err()
            .to_string()
    };
    let small = msg_for(MAX_BYTES + 1);
    let large = msg_for(MAX_BYTES * 4);
    assert!(
        small.contains(&format!("is {} bytes", MAX_BYTES + 1)),
        "{small}"
    );
    assert!(
        large.contains(&format!("is {} bytes", MAX_BYTES * 4)),
        "{large}"
    );
}

#[test]
fn stdin_read_error_surfaces_stdin_read() {
    /// A `Read` impl that always returns an `io::Error` so the
    /// stdin-read branch is exercisable without a closed fd.
    struct BrokenReader;
    impl Read for BrokenReader {
        fn read(&mut self, _: &mut [u8]) -> io::Result<usize> {
            Err(io::Error::other("stdin pipe broken"))
        }
    }
    let mut stdin = BrokenReader;
    let mut stdout = Vec::new();
    let err = run(&mut stdin, &mut stdout, &mut Vec::new()).unwrap_err();
    assert!(matches!(err, Error::StdinRead(_)), "{err}");
}

#[test]
fn stdout_write_error_surfaces_write() {
    /// A `Write` that always errors so the final stdout-write branch
    /// is exercisable without an SIGPIPE setup.
    struct BrokenWriter;
    impl Write for BrokenWriter {
        fn write(&mut self, _: &[u8]) -> io::Result<usize> {
            Err(io::Error::other("stdout pipe closed"))
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }
    let f = NamedTempFile::new().unwrap();
    std::fs::write(f.path(), b"non-empty").unwrap();
    let mut stdin = Cursor::new(input_for(f.path()));
    let mut stdout = BrokenWriter;
    let err = run(&mut stdin, &mut stdout, &mut Vec::new()).unwrap_err();
    assert!(matches!(err, Error::Write(_)), "{err}");
}
