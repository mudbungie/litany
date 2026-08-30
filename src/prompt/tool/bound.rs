//! Head+tail stream bounding for the §3.3 **bounded transcript
//! projection** (bl-d5fa).
//!
//! The executor captures a tool's stdout and stderr in full and lands
//! them in the diagnostic `output.json` (§3.3 Disk record); the
//! transcript entry the model reads is a *projection* of that record,
//! and this module is where the projection is bounded. Each stream is
//! bounded **independently** — the result envelope's `Exit code:`
//! header and `--- stderr ---` marker are structure, never cappable
//! content — to its first `head_bytes` and last `tail_bytes`: the head
//! keeps the command banner (what ran), the tail keeps the failure end
//! (how it ended), the two parts a model acts on. The omitted middle is
//! replaced by an honest marker stating the original byte and line
//! counts and where the full record lives, so the model knows what it
//! lost and can re-run with a filter instead of guessing.
//!
//! Counts are **bytes**, never tokens: litany has no tokenizer, and a
//! fabricated token count would be a lie in the transcript. The split
//! is byte-exact — a multi-byte UTF-8 sequence cut at the boundary
//! degrades to replacement characters in the committed entry, which is
//! the same lossy-UTF-8 discipline the record itself uses (§3.3).

use crate::config::ToolOutputBound;
use std::borrow::Cow;
use std::path::Path;

/// Bound one captured stream for the transcript projection. `label`
/// names the stream in the marker (`stdout` / `stderr`); `record` is
/// the workspace-relative path of the call's `output.json`, where the
/// full bytes live. `None` — the `tool_output:` block absent from the
/// governing `workflow.yaml` — passes the stream through unbounded, as
/// does any stream that fits within `head_bytes + tail_bytes`: the
/// unbounded case is the general path with the policy absent, and a
/// marker is only ever emitted when bytes were actually cut.
pub(super) fn apply<'a>(
    stream: &'a [u8],
    label: &str,
    bound: Option<ToolOutputBound>,
    record: &Path,
) -> Cow<'a, [u8]> {
    let Some(bound) = bound else {
        return Cow::Borrowed(stream);
    };
    let keep = bound.head_bytes.saturating_add(bound.tail_bytes);
    if stream.len() <= keep {
        return Cow::Borrowed(stream);
    }
    let head = &stream[..bound.head_bytes];
    let tail = &stream[stream.len() - bound.tail_bytes..];
    let (total, lines) = (stream.len(), line_count(stream));
    let (head_bytes, tail_bytes) = (bound.head_bytes, bound.tail_bytes);
    let record = record.display();
    let marker = format!(
        "[... {label} truncated: {total} bytes / {lines} lines total; showing the first \
         {head_bytes} and last {tail_bytes} bytes; full record: {record} ...]\n"
    );
    let mut out = Vec::with_capacity(keep + marker.len() + 1);
    out.extend_from_slice(head);
    // The marker is only meaningful on its own line — a head cut
    // mid-line gains a separator rather than fusing with it.
    if !out.is_empty() && !out.ends_with(b"\n") {
        out.push(b'\n');
    }
    out.extend_from_slice(marker.as_bytes());
    out.extend_from_slice(tail);
    Cow::Owned(out)
}

/// Lines in `bytes`, counted the way an editor would: one per newline,
/// plus one for a trailing unterminated line. Empty input has none.
fn line_count(bytes: &[u8]) -> usize {
    let newlines = bytes.iter().filter(|b| **b == b'\n').count();
    match bytes.last() {
        Some(b'\n') | None => newlines,
        Some(_) => newlines + 1,
    }
}

#[cfg(test)]
mod tests {
    use super::{apply, line_count};
    use crate::config::ToolOutputBound;
    use std::borrow::Cow;
    use std::path::Path;

    fn bound(head_bytes: usize, tail_bytes: usize) -> Option<ToolOutputBound> {
        Some(ToolOutputBound {
            head_bytes,
            tail_bytes,
        })
    }

    fn record() -> &'static Path {
        Path::new("steps/a/007/tools/toolu_1/output.json")
    }

    /// No `tool_output:` block — the stream passes through borrowed,
    /// byte-for-byte.
    #[test]
    fn no_policy_is_a_pass_through() {
        let big = vec![b'x'; 4096];
        assert!(matches!(
            apply(&big, "stdout", None, record()),
            Cow::Borrowed(s) if s == big.as_slice()
        ));
    }

    /// A stream within the allowance is untouched — no marker announces
    /// bytes that were not cut. Exactly at the allowance counts as within.
    #[test]
    fn a_fitting_stream_is_untouched() {
        let s = b"1234567890";
        assert!(matches!(
            apply(s, "stdout", bound(5, 5), record()),
            Cow::Borrowed(out) if out == s
        ));
    }

    /// The core shape: head, marker on its own line, tail — with the
    /// marker stating byte count, line count, the split, and the record.
    #[test]
    fn an_oversized_stream_keeps_head_and_tail_around_an_honest_marker() {
        let s = b"AAAA\nmiddle-middle-middle\nZZZZ\n";
        let out = apply(s, "stdout", bound(5, 5), record());
        let text = std::str::from_utf8(&out).unwrap();
        assert_eq!(
            text,
            "AAAA\n[... stdout truncated: 31 bytes / 3 lines total; showing the first \
             5 and last 5 bytes; full record: steps/a/007/tools/toolu_1/output.json ...]\nZZZZ\n"
        );
    }

    /// A head cut mid-line gains a separating newline so the marker
    /// stays on its own line.
    #[test]
    fn a_mid_line_head_cut_gains_a_separator() {
        let s = b"abcdefghijklmnop";
        let out = apply(s, "stderr", bound(4, 4), record());
        let text = std::str::from_utf8(&out).unwrap();
        assert!(text.starts_with("abcd\n[... stderr truncated: 16 bytes / 1 lines"));
        assert!(text.ends_with("...]\nmnop"));
    }

    /// `head_bytes: 0` is legal — tail-only, the marker leading. No
    /// separator is inserted before a marker with nothing ahead of it.
    #[test]
    fn a_zero_head_leads_with_the_marker() {
        let s = b"abcdefgh\n";
        let out = apply(s, "stdout", bound(0, 4), record());
        let text = std::str::from_utf8(&out).unwrap();
        assert!(text.starts_with("[... stdout truncated: 9 bytes"));
        assert!(text.ends_with("...]\nfgh\n"));
    }

    /// Lines are counted the way an editor counts them.
    #[test]
    fn line_counting_matches_an_editor() {
        assert_eq!(line_count(b""), 0);
        assert_eq!(line_count(b"one"), 1);
        assert_eq!(line_count(b"one\n"), 1);
        assert_eq!(line_count(b"one\ntwo"), 2);
        assert_eq!(line_count(b"one\ntwo\n"), 2);
    }
}
