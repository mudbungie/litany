//! Grammar for the `apply_patch` envelope (ARCH §3.3 *The patch tool*).
//!
//! The grammar is codex's `apply_patch` — chosen because models are
//! trained on it, not because it is elegant to parse (the ball's own
//! criterion). One envelope opens with `*** Begin Patch`, closes with
//! `*** End Patch`, and carries any number of file sections:
//!
//! - `*** Add File: <path>` — every following `+`-prefixed line is file
//!   content, and a section carrying none of them is declined (bl-c4a2):
//!   the payload *is* the `+` lines, so zero of them is a malformed
//!   section, not a request for an empty file.
//! - `*** Delete File: <path>` — one line, no body.
//! - `*** Update File: <path>` — **one header per file**, a repeat for a
//!   path the envelope already opened being declined (bl-517e);
//!   optionally `*** Move to: <path>` on the
//!   next line, then one or more hunks: `@@` separates hunks, `@@ <text>`
//!   names an anchor line to locate first (the "@@ enclosing symbol"
//!   disambiguation), ` `-prefixed lines are context, `-` removals, `+`
//!   additions, and `*** End of File` pins the hunk to the file's end.
//!
//! Blank-line semantics are codex's, ruled by reference against its
//! parser (openai/codex, `codex-rs/apply-patch`), not by taste
//! (bl-fdbb). Inside an update body a bare blank line is an empty
//! context line — models routinely drop the lone space — and blank
//! lines directly after `*** End of File` are ignored. Everywhere else
//! codex declines a bare blank line, and so does this parser: an add
//! body's blank content line must be written as a lone `+`, and blank
//! lines between sections are declined, never skipped. Blank lines
//! around the envelope itself are tolerated. Everything else
//! unrecognized is a typed decline naming the line, never a guess.

use thiserror::Error;

mod section;

const BEGIN: &str = "*** Begin Patch";
const END: &str = "*** End Patch";
const ADD: &str = "*** Add File: ";
const DELETE: &str = "*** Delete File: ";
const UPDATE: &str = "*** Update File: ";
const MOVE: &str = "*** Move to: ";
const EOF_MARK: &str = "*** End of File";

/// A parsed envelope: the file operations in author order.
#[derive(Debug, PartialEq)]
pub struct Patch {
    pub ops: Vec<FileOp>,
}

/// One file operation. Paths are as authored — resolved against the
/// calling agent's current working directory at apply time.
#[derive(Debug, PartialEq)]
pub enum FileOp {
    Add {
        path: String,
        lines: Vec<String>,
    },
    Delete {
        path: String,
    },
    Update {
        path: String,
        move_to: Option<String>,
        hunks: Vec<Hunk>,
    },
}

/// One located edit inside an update: anchors are sought first (each
/// moves the cursor past its match), then `old` is located as a block
/// and replaced by `new`. Context lines appear in both sequences.
#[derive(Debug, Default, PartialEq)]
pub struct Hunk {
    pub anchors: Vec<String>,
    pub old: Vec<String>,
    pub new: Vec<String>,
    pub eof: bool,
}

impl Hunk {
    fn is_blank(&self) -> bool {
        self.anchors.is_empty() && self.old.is_empty() && self.new.is_empty() && !self.eof
    }
}

/// Why the envelope did not parse. Every variant names the offense
/// precisely — the model reads this verbatim and repairs the patch.
#[derive(Debug, Error, PartialEq)]
pub enum Error {
    #[error("patch must start with {BEGIN:?}")]
    MissingBegin,
    #[error("patch must end with {END:?}")]
    MissingEnd,
    #[error("patch contains no file operations")]
    Empty,
    #[error(
        "line {line}: unrecognized patch line {content:?}; inside an update, \
         lines start with ' ' (context), '-' (removal), '+' (addition), or '@@'"
    )]
    BadLine { line: usize, content: String },
    #[error(
        "line {line}: bare blank line; only an update body reads a blank \
         line as empty context — in an add section write a lone '+' for \
         a blank content line"
    )]
    BlankLine { line: usize },
    #[error("line {line}: {MOVE:?} must directly follow a {UPDATE:?} line")]
    MisplacedMove { line: usize },
    #[error("update of {path} has no hunks")]
    EmptyUpdate { path: String },
    #[error(
        "line {line}: a second {UPDATE:?} for {path} — that file is already \
         open in this envelope. One header per file; separate its hunks with \
         '@@'"
    )]
    RepeatedUpdate { line: usize, path: String },
    #[error(
        "add of {path} has no content lines; an add section's whole payload \
         is its '+' lines — write every line of the new file with a leading \
         '+' (a blank content line is a lone '+')"
    )]
    EmptyAdd { path: String },
    #[error("update of {path}: hunk {hunk} changes nothing")]
    NoChange { path: String, hunk: usize },
    #[error("{path} appears in more than one file operation")]
    DuplicatePath { path: String },
}

/// Parse the envelope text. Leading/trailing blank lines are tolerated;
/// the markers themselves are matched exactly.
pub fn parse(text: &str) -> Result<Patch, Error> {
    let lines: Vec<&str> = text.lines().collect();
    let first = lines.iter().position(|l| !l.trim().is_empty());
    let last = lines.iter().rposition(|l| !l.trim().is_empty());
    let (Some(first), Some(last)) = (first, last) else {
        return Err(Error::MissingBegin);
    };
    if lines[first] != BEGIN {
        return Err(Error::MissingBegin);
    }
    if lines[last] != END {
        return Err(Error::MissingEnd);
    }
    check_repeated_update(&lines, first + 1, last)?;
    let mut ops = Vec::new();
    let mut i = first + 1;
    while i < last {
        let line = lines[i];
        if let Some(path) = line.strip_prefix(ADD) {
            let (op, next) = section::parse_add(path, &lines, i + 1, last)?;
            ops.push(op);
            i = next;
        } else if let Some(path) = line.strip_prefix(DELETE) {
            ops.push(FileOp::Delete {
                path: path.to_string(),
            });
            i += 1;
        } else if let Some(path) = line.strip_prefix(UPDATE) {
            let (op, next) = section::parse_update(path, &lines, i + 1, last)?;
            ops.push(op);
            i = next;
        } else if line.strip_prefix(MOVE).is_some() {
            return Err(Error::MisplacedMove { line: i + 1 });
        } else if line.trim().is_empty() {
            // codex declines blank lines between sections (and inside
            // add bodies, which break on them and land here).
            return Err(Error::BlankLine { line: i + 1 });
        } else {
            return Err(Error::BadLine {
                line: i + 1,
                content: line.to_string(),
            });
        }
    }
    if ops.is_empty() {
        return Err(Error::Empty);
    }
    check_duplicates(&ops)?;
    Ok(Patch { ops })
}

/// Decline a **second `*** Update File:` header for a path this envelope
/// has already opened** ([`Error::RepeatedUpdate`], bl-517e).
///
/// It is read off the header lines before the section walk, because both
/// orderings of the mistake otherwise decline in a voice that describes a
/// section the model does not believe it wrote. One header per hunk is a
/// reasonable misreading of a format whose hunks are separated by `@@`,
/// and it was 2 of 3 first-patch attempts by the shipped worker model: a
/// second header directly under the first closes an empty section, so the
/// refusal was `update of <path> has no hunks` — true, and about the
/// wrong line — while a second header *after* real hunks parsed twice and
/// refused as [`Error::DuplicatePath`], which names the collision and not
/// the cause. Naming the cause costs one sentence and no second attempt.
///
/// A header is unambiguous at column 0: every line inside an add or an
/// update body carries a `+`, `-`, ` ` or `@@` prefix, which is the same
/// fact [`section::is_section`] already reads.
fn check_repeated_update(lines: &[&str], from: usize, until: usize) -> Result<(), Error> {
    let mut seen = std::collections::BTreeSet::new();
    for (i, line) in lines.iter().enumerate().take(until).skip(from) {
        if let Some(path) = line.strip_prefix(UPDATE)
            && !seen.insert(path)
        {
            return Err(Error::RepeatedUpdate {
                line: i + 1,
                path: path.to_string(),
            });
        }
    }
    Ok(())
}

/// One envelope, one author per path (§2.5 discipline in miniature): a
/// path named by two operations — as a source or as a rename target —
/// would make the result order-dependent, so it is declined.
fn check_duplicates(ops: &[FileOp]) -> Result<(), Error> {
    let mut seen = std::collections::BTreeSet::new();
    for op in ops {
        let paths: Vec<&String> = match op {
            FileOp::Add { path, .. } | FileOp::Delete { path } => vec![path],
            FileOp::Update { path, move_to, .. } => {
                std::iter::once(path).chain(move_to.iter()).collect()
            }
        };
        for path in paths {
            if !seen.insert(path.clone()) {
                return Err(Error::DuplicatePath { path: path.clone() });
            }
        }
    }
    Ok(())
}
