//! `apply_patch` built-in (ARCH §3.3 *The patch tool*): the structured
//! edit path, so file edits stop riding `bash` heredocs and failures
//! become typed declines instead of shell-quoting accidents.
//!
//! Stdin is the `tool_use.input` block as JSON: `{ "input": <string> }`,
//! where the string is one patch envelope in codex's `apply_patch`
//! grammar (`*** Begin Patch` … `*** End Patch`, [`parse`]) carrying
//! add/delete/update/rename for any number of files. The schema field is
//! named `input`, matching the shape those models are tuned against
//! (the same "match a tuned shape" test that governs `bash`, §3.3).
//!
//! Application is **all-or-nothing** ([`apply`]): every operation is
//! validated and every post-state computed in memory before any write
//! lands, so a patch that cannot apply in full applies not at all.
//! Hunks locate their context through the four-rung **matching ladder**
//! ([`seek`]) and the target must be unique at the winning rung —
//! ambiguity and staleness are loud typed declines, never guessed edits
//! (bl-e249). Success prints a JSON report (per file: op, and per hunk
//! the rung, landing line, and — when a fuzzy rung won — the lines
//! actually replaced); the executor's ordinary capture lands it in the
//! diagnostic `output.json` beside the verbatim patch in `input.json`,
//! so the pre/post diff and any decline reason ride the ordinary tool
//! record. Paths resolve against the calling agent's current working
//! directory — the tool subprocess runs there (§3.3 *Working
//! directory*) — and the worktree side effects ride the ordinary
//! per-invocation `git add -A` tool commit, exactly as for `bash`.
//!
//! **Every answer names that directory** (bl-13e2): the report carries
//! `root`, and an apply decline states it after the reason. The paths a
//! report echoes are as authored, so a relative one says nothing about
//! where it landed — and the agent's cwd is the one root its
//! file-touching tools share, moved only by the `cd` tool (a `cd` inside
//! a `bash` command moves that one command's shell and dies with it).

pub mod apply;
pub mod parse;
pub mod report;
pub mod seek;
pub mod splice;

use serde::Deserialize;
use std::io::{self, Read, Write};
use thiserror::Error;

/// Wire shape of the input. `deny_unknown_fields` so a malformed
/// `tool_use.input` surfaces as [`Error::InvalidJson`] rather than
/// silently dropping fields the model meant to pass.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Input {
    input: String,
}

/// Every way [`run`] can fail. Parse and apply declines carry their
/// own precise reasons ([`parse::Error`], [`apply::Error`]); the §3.3
/// stdio contract carries the message into `tool_result.content`
/// verbatim, so the model reads the exact refusal.
#[derive(Debug, Error)]
pub enum Error {
    /// Stdin handed back bytes that did not parse as the documented
    /// shape — wrong type, missing `input`, or extra fields.
    #[error("invalid input JSON: {0}")]
    InvalidJson(#[source] serde_json::Error),
    /// The harness's stdin pipe failed mid-read.
    #[error("read input from stdin: {0}")]
    StdinRead(#[source] io::Error),
    /// The process's working directory could not be resolved — the
    /// anchor every relative patch path needs.
    #[error("resolve working directory: {0}")]
    Cwd(#[source] io::Error),
    /// The envelope did not parse ([`parse::Error`]).
    #[error(transparent)]
    Parse(#[from] parse::Error),
    /// The patch parsed but was refused or failed at application
    /// ([`apply::Error`]) — nothing was written unless the message says
    /// a write itself failed.
    ///
    /// **The working directory travels with it** (bl-13e2). Every arm of
    /// [`apply::Error`] names a path as the patch authored it, which is
    /// usually relative; the model reading the refusal has usually just
    /// been reading absolute paths, and `add REPORT.md: file already
    /// exists` about a file it has watched `rm` remove reads as a broken
    /// tool rather than as two different directories. The root is the
    /// half that was missing, and it is stated once around the whole
    /// decline rather than threaded through nine variants — the failure
    /// is never about *which* path, it is about which root.
    #[error("{source} (working directory: {root})")]
    Apply {
        root: String,
        #[source]
        source: apply::Error,
    },
    /// Writing the report to stdout failed — a harness-side pipe
    /// fault, not a patch failure.
    #[error("write to stdout: {0}")]
    Stdout(#[source] io::Error),
}

/// Read the input, parse the envelope, apply it against the process's
/// working directory, and print the JSON report. Pure over
/// [`Read`]/[`Write`] like its siblings; the cwd is the one ambient
/// fact, pinned by the executor before spawn (§3.3).
pub fn run<R: Read, W: Write>(stdin: &mut R, stdout: &mut W) -> Result<(), Error> {
    let root = std::env::current_dir().map_err(Error::Cwd)?;
    let mut buf = Vec::new();
    stdin.read_to_end(&mut buf).map_err(Error::StdinRead)?;
    let input: Input = serde_json::from_slice(&buf).map_err(Error::InvalidJson)?;
    let patch = parse::parse(&input.input)?;
    let report = apply::apply(&patch, &root).map_err(|source| Error::Apply {
        root: root.display().to_string(),
        source,
    })?;
    let rendered = serde_json::to_string(&report).expect("report serializes");
    stdout.write_all(rendered.as_bytes()).map_err(Error::Stdout)
}

#[cfg(test)]
mod tests;
