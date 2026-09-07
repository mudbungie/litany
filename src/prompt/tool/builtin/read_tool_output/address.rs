//! The **continuation address** grammar (ARCH §3.3 *Paging a cut
//! capture*): the one place it is minted and the one place it is parsed.
//!
//! An address is a workspace-relative record path with a stream and a
//! byte offset appended:
//!
//! ```text
//! steps/<agent-id>/<NNN>/tools/<tool-id>/output.json#<stream>@<offset>
//! ```
//!
//! Nothing is stored to mint one and nothing is looked up to redeem one
//! — every component is already a fact of the tool call the marker was
//! rendered for, so the address *is* the index (`docs/PRINCIPLES.md`,
//! single source of truth: do not store what you can compute).
//!
//! **The agent id is the domain bound, structurally.** It is a segment
//! the grammar cannot omit, so the tool's one string equality against
//! the caller's own `LITANY_CONV_BRANCH` is the whole confinement — no
//! allowlist, nothing to keep in step. The same strictness is the
//! traversal guard: six segments with three of them literal and `<NNN>`
//! all digits admits no `..` and no absolute path at all.
//!
//! Why *address* and not "pagination token": in this tree a **token** is
//! a model token — §3.3 states cut sizes in bytes precisely because
//! "litany has no tokenizer, and a fabricated token count would be a lie
//! in the transcript" — and `address` is already the corpus's word for a
//! followable pointer (`search_history`'s `<commit>:<path>`, §4).

use crate::prompt::step::STEPS_DIR;
use crate::prompt::tool::{OUTPUT_FILE, STEP_TOOLS_SUBDIR};

/// The two captured streams an address can name — the same two labels
/// the bounded projection marks its cuts with (`prompt::tool::bound`).
pub(crate) const STREAMS: [&str; 2] = ["stdout", "stderr"];

/// Separates the record path from the stream, and the stream from the
/// offset. Neither character can occur in a path segment the grammar
/// admits, so the split is unambiguous in both directions.
const STREAM_SEP: char = '#';
const OFFSET_SEP: char = '@';

/// Render an address for `record` — the workspace-relative
/// `…/output.json` path the marker already names — at `offset` bytes
/// into `stream`. The one renderer: the cut marker calls it to mint the
/// first address, and [`Address::at`] calls it for every later page, so
/// the two cannot spell the grammar differently.
pub(crate) fn mint(record: &std::path::Path, stream: &str, offset: usize) -> String {
    format!(
        "{}{STREAM_SEP}{stream}{OFFSET_SEP}{offset}",
        record.display()
    )
}

/// A parsed address. The record path is kept as parsed rather than
/// rebuilt from its segments, so a later page re-renders the very
/// string the marker handed out.
pub(crate) struct Address {
    record: String,
    /// The `<agent-id>` segment — what the tool compares against the
    /// caller's own id before it opens anything.
    pub(crate) agent_id: String,
    /// `stdout` or `stderr`, validated against [`STREAMS`].
    pub(crate) stream: String,
    /// Byte offset into that stream **as the record stores it**.
    pub(crate) offset: usize,
}

impl Address {
    /// Parse `text`, or `None` when it is not the grammar. Total and
    /// allocation-light: every rejection is a shape the tool declines
    /// by naming the grammar, never by guessing what was meant.
    pub(crate) fn parse(text: &str) -> Option<Self> {
        let (record, fragment) = text.rsplit_once(STREAM_SEP)?;
        let (stream, offset) = fragment.rsplit_once(OFFSET_SEP)?;
        if !STREAMS.contains(&stream) {
            return None;
        }
        let offset: usize = offset.parse().ok()?;
        let segments: Vec<&str> = record.split('/').collect();
        let [steps, agent_id, step, tools, tool_id, output] = segments.as_slice() else {
            return None;
        };
        let literals = *steps == STEPS_DIR && *tools == STEP_TOOLS_SUBDIR && *output == OUTPUT_FILE;
        let named = plain(agent_id) && plain(tool_id);
        let numbered = !step.is_empty() && step.bytes().all(|b| b.is_ascii_digit());
        (literals && named && numbered).then(|| Self {
            record: record.to_string(),
            agent_id: (*agent_id).to_string(),
            stream: stream.to_string(),
            offset,
        })
    }

    /// The workspace-relative record path this address names.
    pub(crate) fn record(&self) -> &str {
        &self.record
    }

    /// This address moved to `offset` — the next page's address, in the
    /// same spelling ([`mint`]).
    pub(crate) fn at(&self, offset: usize) -> String {
        mint(std::path::Path::new(&self.record), &self.stream, offset)
    }
}

/// A segment that names something: non-empty, and neither of the two
/// path segments that mean "somewhere else".
fn plain(segment: &str) -> bool {
    !segment.is_empty() && segment != "." && segment != ".."
}
