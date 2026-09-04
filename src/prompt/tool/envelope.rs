//! The **result envelope**: how a finished tool call is rendered for the
//! model (ARCH §3.3 *Result envelope*).
//!
//! The executor holds three facts about a finished child — its exit
//! code, its stdout and its stderr — and the wire `tool_result` carries
//! one content string plus an `is_error` flag. This module is the one
//! place that turns the former into the latter, so what the model reads
//! is a single derivation from the capture rather than a shape assembled
//! across the executor and the step driver.
//!
//! Why the exit code is *stated* rather than left to `is_error`
//! (bl-ffc5): `is_error` is one bit, so a model reading it cannot tell
//! exit 1 (the command ran and failed) from exit 127 (the command does
//! not exist) from exit 143 (the harness cancelled it, §2.9) — three
//! different next moves. It is also the least reliable field on the
//! wire, since each provider protocol spells it differently; the content
//! is round-tripped verbatim by every one of them. Codex, the harness
//! gpt-5.x models are tuned against, states the code in the content for
//! exactly this reason, and those models are trained to read it there.
//!
//! Why stderr is surfaced on success too: a command that exits 0 while
//! writing to stderr is the ordinary case for compilers, test runners
//! and anything that logs progress — dropping those bytes hid warnings
//! and deprecations from the agent, and left `2>&1` as the only way to
//! see them. Streams stay *labelled* rather than merged because the
//! capture holds them apart (`subprocess.rs` reads two pipes) and
//! merging would discard that: a tool whose stdout is a JSON product
//! (`load_skill`'s `{status, path}`, `cd`'s `{cwd}`) must not have a
//! diagnostic line spliced into it.

use crate::config::ToolOutputBound;
use std::path::Path;

/// Fences the stderr block off from the tool's output. On its own line,
/// and only ever emitted with stderr bytes following it.
const STDERR_MARKER: &str = "--- stderr ---\n";

/// Closes a context file's §5.3 frame. On its own line, like the tag it
/// answers.
const FRAME_CLOSE: &str = "</file>\n";

/// Names an appended context file's stream in a bounding marker
/// ([`super::bound::apply`]) — the stream is the file, not a captured
/// pipe, so it is labelled as what it is.
const CONTEXT_FILE_LABEL: &str = "context file";

/// The §5.3 path frame's opening tag for `path`. The one rendering of
/// it: the append below writes it, and the tool window's "already
/// shown" query looks for it in the committed transcript (ARCH §3.3
/// *Context files ride the next tool result*), so a single home is what
/// keeps the two from disagreeing about what a framed file looks like.
pub(in crate::prompt) fn frame_open(path: &Path) -> String {
    format!("<file path=\"{}\">", path.display())
}

/// Render one **context file** as the tail of a tool result (ARCH §3.3
/// *Context files ride the next tool result*): the §5.3 path frame
/// around the file's bytes, the bytes first bounded by the governing
/// `tool_output:` policy **as their own stream** — a context file is
/// neither stdout nor stderr, and bounding it with them would let a
/// long `AGENTS.md` eat the allowance the tool's own output needs.
/// `path` is absolute: it is what the model would hand `read_file`, and
/// what the shown query matches on.
pub(in crate::prompt) fn context_file(
    path: &Path,
    bytes: &[u8],
    bound: Option<ToolOutputBound>,
) -> Vec<u8> {
    let body = super::bound::apply(bytes, CONTEXT_FILE_LABEL, bound, path);
    let mut out = frame_open(path).into_bytes();
    out.push(b'\n');
    out.extend_from_slice(&body);
    // The closing tag is only meaningful on its own line — a file with
    // no trailing newline gains a separator rather than fusing with it.
    if !out.ends_with(b"\n") {
        out.push(b'\n');
    }
    out.extend_from_slice(FRAME_CLOSE.as_bytes());
    out
}

/// Render the model-facing bytes of one finished tool call.
///
/// The shape is: the exit-code line always, the child's stdout verbatim,
/// then — when the child wrote any — a marked stderr block. An empty
/// stream contributes nothing, which is why the marker is conditional:
/// it announces bytes, and there are none to announce. The exit-code
/// line is unconditional because it is the fact the model cannot obtain
/// any other way, and it also means a silent command no longer renders
/// as empty content (a block some providers refuse outright).
pub(super) fn render(exit_code: i32, stdout: &[u8], stderr: &[u8]) -> Vec<u8> {
    let mut out = format!("Exit code: {exit_code}\n").into_bytes();
    out.extend_from_slice(stdout);
    if !stderr.is_empty() {
        if !out.ends_with(b"\n") {
            out.push(b'\n');
        }
        out.extend_from_slice(STDERR_MARKER.as_bytes());
        out.extend_from_slice(stderr);
    }
    out
}

/// Read back the exit code a rendered envelope **states** — the
/// inverse of [`render`]'s first line, and the door verb's own exit
/// code (`docs/DESIGN_CODE_EXECUTION.md` §2.1: "exits with the tool's
/// exit code").
///
/// The envelope is the read, rather than a second field on
/// [`crate::prompt::tool::ToolOutcome`], because most outcomes have no
/// child and therefore no code at all — a grant decline, a control
/// refusal, the multi-tool's aggregate — so a field would be absent
/// exactly where the flag beside it is not, two homes for one fact
/// (`docs/PRINCIPLES.md`). Reading it here keeps the verb's two
/// products, the bytes it prints and the status it exits with, one
/// derivation that cannot disagree.
///
/// `None` for content this module did not render — unreachable through
/// the executor, which renders every outcome it returns, and kept total
/// rather than assumed.
pub(crate) fn stated_exit_code(content: &[u8]) -> Option<i32> {
    let line = content
        .split(|byte| *byte == b'\n')
        .next()
        .unwrap_or_default();
    std::str::from_utf8(line)
        .ok()?
        .strip_prefix("Exit code: ")?
        .parse()
        .ok()
}

#[cfg(test)]
mod tests {
    use super::{context_file, frame_open, render, stated_exit_code};
    use crate::config::ToolOutputBound;
    use std::path::Path;

    fn framed(path: &str, body: &str, bound: Option<ToolOutputBound>) -> String {
        String::from_utf8(context_file(Path::new(path), body.as_bytes(), bound))
            .expect("ASCII fixtures render as UTF-8")
    }

    /// The §5.3 frame is the one the assembler writes for every other
    /// file the model is shown, so a context file reads like one.
    #[test]
    fn a_context_file_rides_in_the_path_frame() {
        assert_eq!(
            framed("/w/proj/AGENTS.md", "house rules\n", None),
            "<file path=\"/w/proj/AGENTS.md\">\nhouse rules\n</file>\n"
        );
        assert_eq!(
            frame_open(Path::new("/w/AGENTS.md")),
            "<file path=\"/w/AGENTS.md\">"
        );
    }

    /// A file with no trailing newline must not fuse with its closing
    /// tag — the tag is only meaningful on its own line.
    #[test]
    fn an_unterminated_file_gains_a_separator_before_the_close() {
        assert_eq!(
            framed("/w/AGENTS.md", "no newline", None),
            "<file path=\"/w/AGENTS.md\">\nno newline\n</file>\n"
        );
    }

    /// The file is bounded as **its own** stream: the `tool_output:`
    /// allowance applies to it whole, so a long `AGENTS.md` cannot eat
    /// the allowance the tool's own output needs — and the marker names
    /// the file itself as where the full record lives.
    #[test]
    fn an_oversized_context_file_is_bounded_as_its_own_stream() {
        let bound = Some(ToolOutputBound {
            head_bytes: 5,
            tail_bytes: 5,
        });
        let out = framed("/w/AGENTS.md", "AAAA\nmiddle-middle\nZZZZ\n", bound);
        assert!(
            out.starts_with("<file path=\"/w/AGENTS.md\">\nAAAA\n[... context file truncated: ")
        );
        assert!(out.contains("full record: /w/AGENTS.md ...]\nZZZZ\n</file>\n"));
    }

    fn rendered(exit_code: i32, stdout: &str, stderr: &str) -> String {
        String::from_utf8(render(exit_code, stdout.as_bytes(), stderr.as_bytes()))
            .expect("ASCII fixtures render as UTF-8")
    }

    /// The common case costs exactly one line: the code, then the output.
    #[test]
    fn success_states_the_code_and_carries_stdout() {
        assert_eq!(rendered(0, "hello\n", ""), "Exit code: 0\nhello\n");
    }

    /// The defect bl-ffc5 names: stderr on a zero exit used to be
    /// dropped, so a warning on a successful build was invisible.
    #[test]
    fn stderr_survives_a_zero_exit() {
        assert_eq!(
            rendered(0, "built\n", "warning: deprecated\n"),
            "Exit code: 0\nbuilt\n--- stderr ---\nwarning: deprecated\n"
        );
    }

    /// The other half: exit 1 and exit 127 are distinguishable, where
    /// `is_error` alone made them the same bit.
    #[test]
    fn the_exit_code_distinguishes_failures() {
        assert_eq!(
            rendered(127, "", "sh: nope: not found\n"),
            "Exit code: 127\n--- stderr ---\nsh: nope: not found\n"
        );
        assert_eq!(
            rendered(1, "", "assertion failed\n"),
            "Exit code: 1\n--- stderr ---\nassertion failed\n"
        );
    }

    /// Stdout that does not end in a newline must not run into the
    /// marker — the marker is only meaningful on its own line.
    #[test]
    fn unterminated_stdout_gains_a_separator_before_the_marker() {
        assert_eq!(
            rendered(0, "no trailing newline", "note\n"),
            "Exit code: 0\nno trailing newline\n--- stderr ---\nnote\n"
        );
    }

    /// No stderr, no marker: an empty stream announces nothing.
    #[test]
    fn an_empty_stderr_emits_no_marker() {
        let out = rendered(3, "partial", "");
        assert_eq!(out, "Exit code: 3\npartial");
        assert!(!out.contains("stderr"));
    }

    /// A silent command still renders as content, so no tool call
    /// produces an empty `tool_result` block.
    #[test]
    fn a_silent_command_is_not_empty_content() {
        assert_eq!(rendered(0, "", ""), "Exit code: 0\n");
    }

    /// Bytes are passed through, not transcoded: the executor's raw
    /// capture reaches the model as the tool wrote it.
    #[test]
    fn non_utf8_bytes_pass_through_untouched() {
        let out = render(0, &[0xff, 0xfe], &[0x80]);
        assert_eq!(out, b"Exit code: 0\n\xff\xfe\n--- stderr ---\n\x80");
    }

    /// The stated code reads back off the rendered bytes — the door
    /// verb's exit status is the envelope's own first line.
    #[test]
    fn the_stated_code_reads_back_off_a_rendered_envelope() {
        for code in [0, 3, 127, 143] {
            let rendered = render(code, b"out", b"err");
            assert_eq!(stated_exit_code(&rendered), Some(code));
        }
    }

    /// Total over bytes this module did not render: no first line to
    /// read, a line that is not the header, or a header whose tail is
    /// not a number all answer `None` rather than a wrong code.
    #[test]
    fn content_that_is_not_an_envelope_states_no_code() {
        for content in ["", "hello\nworld", "Exit code: seven\n"] {
            assert_eq!(stated_exit_code(content.as_bytes()), None, "{content:?}");
        }
    }
}
