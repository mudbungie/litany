//! The two **section bodies** of the `apply_patch` grammar: an add's `+`
//! content and an update's hunks (ARCH §3.3 *The patch tool*).
//!
//! [`super`] walks the envelope and owns what a patch *is* — the ops, the
//! hunk, the declines; this file reads the lines between one section
//! header and the next. Split out at bl-c4a2 when the two grew past the
//! per-file cap together: the envelope walk and a section body change for
//! different reasons and were already separated by a function boundary.

use super::{ADD, DELETE, END, EOF_MARK, Error, FileOp, Hunk, MOVE, UPDATE};

/// True when `line` opens a new file section (or is the `Move to` rider).
pub(super) fn is_section(line: &str) -> bool {
    [ADD, DELETE, UPDATE, MOVE]
        .iter()
        .any(|m| line.starts_with(m))
}

/// Collect an add section's `+`-prefixed content. Returns the op and the
/// index of the first line past the section. A bare blank line is not
/// content (codex declines it — a blank content line is a lone `+`); it
/// ends the section and the caller declines it as [`Error::BlankLine`].
///
/// **A section with no `+` lines at all is declined** ([`Error::EmptyAdd`],
/// bl-c4a2). The section's whole payload is its `+` lines, so zero of them
/// is a malformed section rather than a request for an empty file — and
/// accepting it was worse than useless: it wrote a 0-byte file, answered
/// with the `applied` receipt that stops a model retrying, and took the
/// name, so the very next add of the same path hit `AddExists` and the
/// model read its own grammar as broken. An empty file is `bash`'s
/// (`: > path`), which says what it means.
pub(super) fn parse_add(
    path: &str,
    lines: &[&str],
    from: usize,
    until: usize,
) -> Result<(FileOp, usize), Error> {
    let mut content = Vec::new();
    let mut i = from;
    while i < until {
        if let Some(rest) = lines[i].strip_prefix('+') {
            content.push(rest.to_string());
        } else {
            break;
        }
        i += 1;
    }
    if content.is_empty() {
        return Err(Error::EmptyAdd {
            path: path.to_string(),
        });
    }
    let op = FileOp::Add {
        path: path.to_string(),
        lines: content,
    };
    Ok((op, i))
}

/// Collect an update section: the optional `Move to` rider, then hunks.
pub(super) fn parse_update(
    path: &str,
    lines: &[&str],
    from: usize,
    until: usize,
) -> Result<(FileOp, usize), Error> {
    let mut i = from;
    let mut move_to = None;
    if i < until
        && let Some(to) = lines[i].strip_prefix(MOVE)
    {
        move_to = Some(to.to_string());
        i += 1;
    }
    let mut hunks: Vec<Hunk> = Vec::new();
    let mut cur = Hunk::default();
    let mut flush = |cur: &mut Hunk| -> Result<(), Error> {
        let hunk = std::mem::take(cur);
        if hunk.is_blank() {
            return Ok(());
        }
        if hunk.old == hunk.new {
            return Err(Error::NoChange {
                path: path.to_string(),
                hunk: hunks.len() + 1,
            });
        }
        hunks.push(hunk);
        Ok(())
    };
    let mut after_eof = false;
    while i < until && !is_section(lines[i]) && lines[i] != END {
        let line = lines[i];
        if line == EOF_MARK {
            cur.eof = true;
            flush(&mut cur)?;
            after_eof = true;
        } else if line.is_empty() {
            // codex ignores blank lines directly after `*** End of
            // File`; elsewhere in an update body a bare blank line is
            // an empty context line.
            if !after_eof {
                cur.old.push(String::new());
                cur.new.push(String::new());
            }
        } else if line == "@@" {
            flush(&mut cur)?;
            after_eof = false;
        } else if let Some(anchor) = line.strip_prefix("@@ ") {
            // An anchor after body lines opens the next hunk.
            if !cur.old.is_empty() || !cur.new.is_empty() {
                flush(&mut cur)?;
            }
            cur.anchors.push(anchor.to_string());
            after_eof = false;
        } else if let Some(rest) = line.strip_prefix('+') {
            cur.new.push(rest.to_string());
            after_eof = false;
        } else if let Some(rest) = line.strip_prefix('-') {
            cur.old.push(rest.to_string());
            after_eof = false;
        } else if let Some(rest) = line.strip_prefix(' ') {
            cur.old.push(rest.to_string());
            cur.new.push(rest.to_string());
            after_eof = false;
        } else {
            return Err(Error::BadLine {
                line: i + 1,
                content: line.to_string(),
            });
        }
        i += 1;
    }
    flush(&mut cur)?;
    if hunks.is_empty() {
        return Err(Error::EmptyUpdate {
            path: path.to_string(),
        });
    }
    let op = FileOp::Update {
        path: path.to_string(),
        move_to,
        hunks,
    };
    Ok((op, i))
}
