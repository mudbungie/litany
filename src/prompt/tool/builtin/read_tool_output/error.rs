//! Every way the page read declines (ARCH §3.3 *Paging a cut capture*).
//!
//! Split from [`super`] at the pre-split band: the module it left is
//! *how a page is answered*, this is *what a refusal says*. Each variant
//! prints its own stderr message; per §3.3 stderr concatenates after
//! stdout into `tool_result.content` on a non-zero exit, so the model
//! reads the decline verbatim and can correct itself without a human.

use std::io;
use std::path::PathBuf;
use thiserror::Error;

/// Every way [`run`] can fail. Each prints its own stderr message; per
/// §3.3 stderr concatenates after stdout into `tool_result.content` on a
/// non-zero exit, so the model reads the decline verbatim.
#[derive(Debug, Error)]
pub enum Error {
    #[error("invalid input JSON: {0}")]
    InvalidJson(#[source] serde_json::Error),
    #[error("read input from stdin: {0}")]
    StdinRead(#[source] io::Error),
    #[error("missing env var {0:?} (set by the harness per ARCH §3.3)")]
    MissingEnv(&'static str),
    /// Not the grammar. The decline states it rather than guessing what
    /// was meant — an address is copied from a cut marker, so a
    /// malformed one is a transcription fault with an exact fix.
    #[error(
        "malformed address {given:?}: expected \
         steps/<agent-id>/<NNN>/tools/<tool-id>/output.json#<stdout|stderr>@<byte offset>, \
         copied verbatim from the marker in the cut result"
    )]
    Malformed { given: String },
    /// The address names another agent's captures. Refused by name: the
    /// agent-id segment is the domain bound (ARCH §3.3), so this is the
    /// whole confinement rather than a check beside one.
    #[error(
        "address names agent {theirs:?}, and you are {ours:?}: \
         an agent may page only its own tool captures"
    )]
    ForeignAgent { theirs: String, ours: String },
    /// The record is not on disk (never written, or swept by §9.2
    /// retention), or is not the shape [`ToolOutputRecord`] pins.
    #[error("no capture at {record}: {source}", record = record.display())]
    NoRecord {
        record: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("capture at {record} did not parse: {source}", record = record.display())]
    Unreadable {
        record: PathBuf,
        #[source]
        source: serde_json::Error,
    },
    /// Past the end of the stream. States the total so the next read is
    /// a correction rather than a guess.
    #[error("offset {offset} is past the end of {stream} ({total} bytes)")]
    PastEnd {
        offset: usize,
        stream: String,
        total: usize,
    },
    #[error("write the page: {0}")]
    Write(#[source] io::Error),
}
