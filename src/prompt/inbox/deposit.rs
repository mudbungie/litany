//! Message deposit (ARCH §2.11 *Deposit*).
//!
//! A deposit writes one **create-only new file** into the recipient's
//! inbox at `<workspace>/inbox/<agent-id>/<sender>-<NNN>.md`, via
//! temp-path + atomic rename. `<sender>` is the depositing agent's id or
//! `user`; `<NNN>` is the sender's own sequence, derived (never stored)
//! as max-present-plus-one over the sender's existing files in that
//! inbox. The path carries exactly one fact — framing (the sender) —
//! and every other asserted fact rides the frontmatter (`from:`,
//! `from_name:`, `deposited_at:`); the body is the content verbatim
//! (§2.11).
//!
//! **`from:` is the address and `from_name:` is the reading** (bl-a457).
//! `from:` stays the agent id (or `user`): it is what the filename is
//! namespaced by, what a reply is addressed to, and the handle that still
//! resolves once the agent is dead and its name has been recycled
//! (§2.3). But an id is ~60 characters of timestamped hex and a name is
//! one word, and `dispatch`'s own schema promises the name is "the
//! child's identity in every surface" — so the name a dispatch minted
//! travels beside the id, and every reader of a message (the model whose
//! context it becomes, and any seat rendering it) can attribute by it.
//! The line is present exactly when the sender wears one: `user` never
//! does, an unnamed agent never does, and absence is the general path
//! with an empty input rather than a second shape to interpret.
//!
//! Create-only-ness is structural, not a check: sender-namespacing makes
//! cross-sender collision impossible and a single sender is sequential
//! with itself, so the target name never pre-exists; temp-path + rename
//! then guarantees no reader observes a half-written file.

use super::inbox_dir;
use crate::prompt::Clock;
use crate::template::GitRunner;
use std::io;
use std::path::{Path, PathBuf};

pub(crate) mod result;

pub use result::{Epitaph, deposit_result, returned_ref};

/// Extension of a deposited message file.
const MESSAGE_EXT: &str = "md";
/// Zero-pad width of the `<NNN>` sequence, matching the transcript /
/// step-record 3-digit convention (§2.3).
const SEQ_WIDTH: usize = 3;
/// First sequence number when a sender has no prior files in the inbox.
const FIRST_SEQ: u32 = 1;

/// Why a [`deposit`] could not complete. Every arm is an inbox I/O
/// failure carrying the offending path for a legible operator message.
#[derive(Debug, thiserror::Error)]
pub enum DepositError {
    #[error("inbox i/o at {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    /// The returned mark could not be written after the result file
    /// landed. Surfaced loudly: an unmarked return is exactly the state
    /// the §8 sweep would later misread as a silent death.
    #[error("mark refs/litany/returned/{child}: {source}")]
    Mark {
        child: String,
        #[source]
        source: io::Error,
    },
}

fn io_err(path: &Path, source: io::Error) -> DepositError {
    DepositError::Io {
        path: path.to_path_buf(),
        source,
    }
}

/// Deposit `content` from `sender` into `agent_id`'s inbox under
/// `workspace`. Returns the path of the created message file. `sender`
/// is a caller-supplied agent id or [`USER_SENDER`] — never taken from
/// model input (§2.11: provenance is harness-derived). `git` reads the
/// sender's name off the `agents/*` registry (§2.3) for the frontmatter's
/// `from_name:`; a sender with no ref and no name simply has none.
pub fn deposit(
    workspace: &Path,
    agent_id: &str,
    sender: &str,
    content: &str,
    clock: &dyn Clock,
    git: &dyn GitRunner,
) -> Result<PathBuf, DepositError> {
    let dir = inbox_dir(workspace, agent_id);
    std::fs::create_dir_all(&dir).map_err(|e| io_err(&dir, e))?;
    let seq = next_sequence(&dir, sender).map_err(|e| io_err(&dir, e))?;
    let filename = message_filename(sender, seq);
    let fields = fields(workspace, sender, &clock.now_iso8601(), git);
    let body = format!("---\n{fields}---\n{content}");
    atomic_create(&dir, &filename, body.as_bytes())?;
    Ok(dir.join(filename))
}

/// `<sender>-<NNN>.md` with `NNN` zero-padded to [`SEQ_WIDTH`].
fn message_filename(sender: &str, seq: u32) -> String {
    format!("{sender}-{seq:0width$}.{MESSAGE_EXT}", width = SEQ_WIDTH)
}

/// The sender's next sequence number: max-present-plus-one over the
/// sender's own `<sender>-<NNN>.md` files in `dir`, or [`FIRST_SEQ`] when
/// it has none. Derived from a directory listing, never stored (§2.3
/// "order has one home, the name"). A file that does not match the
/// sender's own prefix-and-numeric shape (another sender's deposit, a
/// stray) is ignored, so senders never miscount each other.
pub(super) fn next_sequence(dir: &Path, sender: &str) -> io::Result<u32> {
    let prefix = format!("{sender}-");
    let suffix = format!(".{MESSAGE_EXT}");
    let mut max: Option<u32> = None;
    // `flatten` drops any per-entry read error: the sequence is derived
    // from whatever files are legibly present, so a transient enumeration
    // failure degrades to (at worst) reusing a number, never a panic.
    for entry in std::fs::read_dir(dir)?.flatten() {
        let name = entry.file_name();
        let Some(name) = name.to_str() else { continue };
        if let Some(seq) = parse_seq(name, &prefix, &suffix) {
            max = Some(max.map_or(seq, |m| m.max(seq)));
        }
    }
    Ok(max.map_or(FIRST_SEQ, |m| m + 1))
}

/// Parse the `<NNN>` out of `<prefix><NNN><suffix>`, requiring the middle
/// to be all digits — so `user-abc.md` under prefix `user-` yields
/// `None`, and a longer-id sender's file (`p1-abc-001.md` under prefix
/// `p1-`) yields `None` because `abc-001` is not numeric.
fn parse_seq(name: &str, prefix: &str, suffix: &str) -> Option<u32> {
    let mid = name.strip_prefix(prefix)?.strip_suffix(suffix)?;
    mid.parse::<u32>().ok()
}

/// The frontmatter fields every deposit asserts — `from:`, the sender's
/// `from_name:` when it wears one, and `deposited_at:` — as lines, with
/// no delimiters around them. The caller wraps them, so a result message
/// adds its own pinned fields inside the same block rather than opening a
/// second one. The one home of what a deposit says about itself.
fn fields(workspace: &Path, sender: &str, deposited_at: &str, git: &dyn GitRunner) -> String {
    // A sender is an agent id or `user` (module docs), and only an agent
    // can wear a name — so the registry is asked about agents and not
    // about the reserved token, which has no `agents/*` ref to read.
    let name = (sender != super::USER_SENDER)
        .then(|| crate::workspace::agent_name::read(workspace, sender, git))
        .flatten();
    let named = match name {
        Some(name) => format!("from_name: {name}\n"),
        None => String::new(),
    };
    format!("from: {sender}\n{named}deposited_at: {deposited_at}\n")
}

/// Write `bytes` to `dir/filename` via a sibling temp file and an atomic
/// rename, so no reader ever observes a partial deposit (§2.11).
pub(super) fn atomic_create(dir: &Path, filename: &str, bytes: &[u8]) -> Result<(), DepositError> {
    let final_path = dir.join(filename);
    let tmp_path = dir.join(format!(".{filename}.tmp"));
    std::fs::write(&tmp_path, bytes).map_err(|e| io_err(&tmp_path, e))?;
    std::fs::rename(&tmp_path, &final_path).map_err(|e| io_err(&final_path, e))?;
    Ok(())
}
