//! The line range one `read_file` invocation returns, and the note that
//! says what it was (`docs/DESIGN_CODE_EXECUTION.md` §3 *read_file
//! offset/limit*).
//!
//! Lines are counted 1-based over the file's bytes: a line ends at its
//! `\n` (which it keeps) or at EOF, so an empty file has **no** lines
//! and a file ending in `\n` has no trailing empty one. That is the same
//! counting `sed -n 'A,Bp'` and `wc -l`-plus-a-tail do, which matters
//! because the note's `offset` is what the model hands back.

/// One selection over a file's bytes: which slice to write, and the
/// three counts the note is derived from. Byte indices rather than an
/// owned copy — the caller already holds the bytes.
pub(super) struct Selection {
    /// Byte index the selected lines begin at.
    pub(super) start: usize,
    /// Byte index one past the selected lines.
    pub(super) end: usize,
    /// The 1-based line the read began at — the request's `offset`,
    /// echoed whether or not the file reaches it.
    pub(super) offset: u64,
    /// How many lines the slice holds.
    pub(super) returned: u64,
    /// How many lines the file holds.
    pub(super) total: u64,
}

impl Selection {
    /// The one sentence the tool writes to stderr, on every read
    /// (§3.3 *Result envelope* surfaces stderr on success too). One
    /// shape for every case — a whole-file read is the ranged read with
    /// empty inputs, so there is no arm for it — with the continuation
    /// clause present exactly when lines remain past the slice.
    pub(super) fn note(&self) -> String {
        let mut note = format!(
            "read_file: {} of {} lines from offset {}",
            self.returned, self.total, self.offset
        );
        let next = self.offset.saturating_add(self.returned);
        if next <= self.total {
            note.push_str(&format!("; continue with offset {next}"));
        }
        note.push('\n');
        note
    }
}

/// Select `limit` lines (all of them when `None`) starting at the
/// 1-based `offset`, and count the file's lines on the way past.
///
/// One pass, no per-line index: the file is already in memory under the
/// [`super::MAX_BYTES`] cap, so the only thing worth avoiding is a
/// second allocation the size of the file.
pub(super) fn select(content: &[u8], offset: u64, limit: Option<u64>) -> Selection {
    // Inclusive last line to keep; `None` is "to the end". Saturating
    // because a model may hand us `limit: u64::MAX`.
    let last = limit.map(|l| offset.saturating_add(l).saturating_sub(1));
    let mut total: u64 = 0;
    let mut returned: u64 = 0;
    let mut start: Option<usize> = None;
    let mut end: usize = 0;
    let mut begin: usize = 0;
    while begin < content.len() {
        let stop = content[begin..]
            .iter()
            .position(|b| *b == b'\n')
            .map_or(content.len(), |p| begin + p + 1);
        total = total.saturating_add(1);
        if total >= offset && last.is_none_or(|l| total <= l) {
            if start.is_none() {
                start = Some(begin);
            }
            end = stop;
            returned = returned.saturating_add(1);
        }
        begin = stop;
    }
    let start = start.unwrap_or(0);
    Selection {
        start,
        end,
        offset,
        returned,
        total,
    }
}
