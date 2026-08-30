//! `read_file` built-in (ARCH §3.3, §12 v0.3 toolset).
//!
//! Stdin is the `tool_use.input` block as JSON: `{ "path": <string> }`.
//! Stdout is the file's raw bytes; exit code 0 on success. Errors land
//! on stderr (the executor concats it after stdout into
//! `tool_result.content` per §3.3) and the process exits non-zero.
//!
//! Oversized files are rejected with [`Error::TooLarge`] rather than
//! truncated. The auto-dispatch shim that turns oversized output into a
//! summarized read is deferred to v0.4+ (epic non-goal in §11/§12).

use serde::Deserialize;
use std::fs::File;
use std::io::{self, Read, Write};
use std::path::PathBuf;
use thiserror::Error;

/// Hard ceiling on bytes the tool will hand back to the model. 1 MiB
/// is comfortably above any single source file the agent would read
/// directly and small enough that the result stays tractable in the
/// model's context window. Tools that need larger reads should
/// reach for `bash` (e.g. `head -n N`) until the v0.4 auto-dispatch
/// path lands.
pub const MAX_BYTES: u64 = 1024 * 1024;

/// Wire shape of the input. `serde` enforces required-and-no-extras so
/// a malformed `tool_use.input` surfaces as [`Error::InvalidJson`]
/// rather than a silent fallback.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Input {
    path: PathBuf,
}

/// Every way [`run`] can fail. Each variant produces a distinct
/// stderr message — the operator running `litany tool read_file`
/// directly sees these on the terminal, and the model sees them
/// concatenated into `tool_result.content` when the executor builds
/// the next step's request (§3.3).
#[derive(Debug, Error)]
pub enum Error {
    /// Stdin handed back bytes that did not parse as the documented
    /// shape — wrong type, missing `path`, or extra fields.
    #[error("invalid input JSON: {0}")]
    InvalidJson(#[source] serde_json::Error),
    /// The harness's stdin pipe failed mid-read. Distinct from
    /// [`Error::InvalidJson`] so a transient pipe failure isn't
    /// misattributed to the model.
    #[error("read input from stdin: {0}")]
    StdinRead(#[source] io::Error),
    /// `stat` or `open` failed — file missing, permission denied,
    /// path is a directory, etc. The path is captured so the message
    /// pinpoints which input was bad.
    #[error("open {path}: {source}", path = path.display())]
    Open {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    /// The file's size exceeds [`MAX_BYTES`]. Reported as a hard
    /// rejection rather than a truncation so the agent sees the cap
    /// and chooses a different tactic. `size` is the file's **true**
    /// size (`stat` on the open fd), not the capped read's length —
    /// an agent deciding between `head -c` and a different tactic
    /// needs the real magnitude.
    #[error(
        "file {path} is {size} bytes (cap {cap}); use a streaming tool",
        path = path.display()
    )]
    TooLarge { path: PathBuf, cap: u64, size: u64 },
    /// `read` returned an I/O error after `open` succeeded — disk
    /// fault, mid-read truncation, etc.
    #[error("read {path}: {source}", path = path.display())]
    Read {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    /// Writing to stdout failed. Only fires when the harness's stdout
    /// pipe is closed before we finish writing — a fault, not a tool
    /// failure delivered to the model.
    #[error("write to stdout: {0}")]
    Write(#[source] io::Error),
}

/// Read the input from `stdin`, open the named file, and write its
/// bytes to `stdout`. Pure over [`Read`]/[`Write`] so unit tests drive
/// it with `Cursor`/`Vec`; the `litany tool read_file` shim wires it
/// to the live process stdio.
pub fn run<R: Read, W: Write>(stdin: &mut R, stdout: &mut W) -> Result<(), Error> {
    let mut buf = Vec::new();
    stdin.read_to_end(&mut buf).map_err(Error::StdinRead)?;
    let input: Input = serde_json::from_slice(&buf).map_err(Error::InvalidJson)?;
    let path = input.path;

    let file = File::open(&path).map_err(|source| Error::Open {
        path: path.clone(),
        source,
    })?;
    // `take(MAX_BYTES + 1)` enforces the cap on the happy path with no
    // `metadata` call: a file at the cap reads MAX_BYTES bytes and
    // succeeds; anything larger trips the post-read length check
    // below, which then (and only then) stats for the true size.
    let mut content = Vec::new();
    let mut capped = file.take(MAX_BYTES + 1);
    capped
        .read_to_end(&mut content)
        .map_err(|source| Error::Read {
            path: path.clone(),
            source,
        })?;
    let read = content.len() as u64;
    if read > MAX_BYTES {
        // The capped read only proves the file is over the cap — its
        // length is `cap + 1` by construction, so reporting it would
        // fabricate the same size for every oversize file. `stat` the
        // already-open fd for the true size, floored at what we read:
        // a stream whose metadata understates its content (procfs
        // reports len 0) still reports at least the bytes seen.
        let size = capped
            .get_ref()
            .metadata()
            .map_or(read, |m| m.len().max(read));
        let cap = MAX_BYTES;
        return Err(Error::TooLarge { path, cap, size });
    }
    stdout.write_all(&content).map_err(Error::Write)
}

#[cfg(test)]
mod tests;
