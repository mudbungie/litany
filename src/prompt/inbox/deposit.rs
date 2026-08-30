//! Message deposit (ARCH §2.11 *Deposit*).
//!
//! A deposit writes one **create-only new file** into the recipient's
//! inbox at `<workspace>/inbox/<agent-id>/<sender>-<NNN>.md`, via
//! temp-path + atomic rename. `<sender>` is the depositing agent's id or
//! `user`; `<NNN>` is the sender's own sequence, derived (never stored)
//! as max-present-plus-one over the sender's existing files in that
//! inbox. The path carries exactly one fact — framing (the sender) —
//! and every other asserted fact rides the frontmatter (`from:`,
//! `deposited_at:`); the body is the content verbatim (§2.11).
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

/// Ref-namespace prefix for the durable **returned** mark,
/// `refs/litany/returned/<child-id>` → the child's terminal ref — written
/// by [`deposit_result`] the moment a result message lands (ARCH §2.6,
/// §8). The fact's one durable home: the message file is consumed by
/// delivery or by a compaction landing, and even its delivered transcript
/// entry can be squashed away by a later compaction — so "this child
/// deposited a result" must outlive every downstream trace, or the §8
/// sweep re-derives a death for a child that returned cleanly. Shares
/// [`crate::workspace::MARK_REF_ROOT`], so §9.2 retention recycles it.
pub const RETURNED_REF_PREFIX: &str = "refs/litany/returned/";

/// The child's returned-mark ref, `refs/litany/returned/<child-id>`.
pub fn returned_ref(child_id: &str) -> String {
    format!("{RETURNED_REF_PREFIX}{child_id}")
}

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
/// model input (§2.11: provenance is harness-derived).
pub fn deposit(
    workspace: &Path,
    agent_id: &str,
    sender: &str,
    content: &str,
    clock: &dyn Clock,
) -> Result<PathBuf, DepositError> {
    let dir = inbox_dir(workspace, agent_id);
    std::fs::create_dir_all(&dir).map_err(|e| io_err(&dir, e))?;
    let seq = next_sequence(&dir, sender).map_err(|e| io_err(&dir, e))?;
    let filename = message_filename(sender, seq);
    let body = render(sender, &clock.now_iso8601(), content);
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

/// The on-disk message body: `from:` / `deposited_at:` frontmatter
/// followed by the content verbatim (§2.11 — frontmatter carries the
/// asserted facts, the body is the content).
fn render(sender: &str, deposited_at: &str, content: &str) -> String {
    format!("---\nfrom: {sender}\ndeposited_at: {deposited_at}\n---\n{content}")
}

/// The pinned manner of an agent's ending, carried by a **result
/// message** (ARCH §2.6). A *total* field — the union over every
/// terminal event, never an exception set — so downstream code branches
/// on its **value**, never on the message's shape (§2.6). The on-disk
/// spelling is hyphenated (`final-response`, `budget-exhausted`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Epitaph {
    /// The agent produced a final response and terminated normally.
    FinalResponse,
    /// The agent was stopped — user, timeout, or parent cascade (§2.9).
    Stopped,
    /// The agent tree exhausted a spend budget (§6).
    BudgetExhausted,
    /// The agent crashed too hard to run any handler; the §8 sweep
    /// deposits this on its behalf (§2.6, §2.9).
    Died,
}

impl Epitaph {
    /// The on-disk `epitaph:` value (§2.6, §2.11).
    pub fn as_str(self) -> &'static str {
        match self {
            Epitaph::FinalResponse => "final-response",
            Epitaph::Stopped => "stopped",
            Epitaph::BudgetExhausted => "budget-exhausted",
            Epitaph::Died => "died",
        }
    }
}

/// Deposit a **result message** (ARCH §2.6) from a terminated agent
/// (`child_id`) into `recipient_id`'s inbox under `workspace`. Who the
/// recipient *is* is decided by the epitaph's value at the executor's
/// own seam ([`crate::prompt::dispatch`] — a reply answers the last
/// prompter, an obituary reports to the dispatcher); this deposit takes
/// the address and writes the file.
/// This is an ordinary [`deposit`] whose frontmatter additionally
/// carries the two pinned fields — `epitaph:` (always) and
/// `terminal_ref:` (always, the sha of the child's branch tip at
/// return) — and whose body is the terminal response iff the agent
/// spoke (`terminal_response` is `Some`); the body is absent exactly
/// when it never spoke (§2.6, §2.11). One file shape, no sidecar, no
/// variant kinds. Sender is the child, so the parent's sender-namespaced
/// inbox records "a message from the child exists" (§2.11) — which is
/// what lets the §8 sweep act as scribe for a crashed child.
///
/// Executor-side by construction: this is a plain filesystem deposit,
/// never a model `message` tool call ("Return is not a verb",
/// `docs/PRINCIPLES.md`). Total and reusable — the normal terminal
/// paths (§2.9, §6) and the §8 silent-death sweep (bl-d148) all deposit
/// through it — which is what makes it the one seam where the durable
/// **returned mark** is written ([`RETURNED_REF_PREFIX`]): every result
/// deposit, whoever makes it, leaves the mark, so the §8 sweep's
/// returned derivation survives the message's later consumption. The
/// mark lands *after* the file: in the crash window between the two the
/// file itself is the evidence (the sweep reads the inbox first), so
/// neither ordering half can strand or double-deposit.
#[allow(clippy::too_many_arguments)] // one deposit, every pinned fact it renders
pub fn deposit_result(
    workspace: &Path,
    recipient_id: &str,
    child_id: &str,
    epitaph: Epitaph,
    terminal_ref: &str,
    terminal_response: Option<&str>,
    clock: &dyn Clock,
    git: &dyn GitRunner,
) -> Result<PathBuf, DepositError> {
    let dir = inbox_dir(workspace, recipient_id);
    std::fs::create_dir_all(&dir).map_err(|e| io_err(&dir, e))?;
    let seq = next_sequence(&dir, child_id).map_err(|e| io_err(&dir, e))?;
    let filename = message_filename(child_id, seq);
    let body = render_result(
        child_id,
        &clock.now_iso8601(),
        epitaph,
        terminal_ref,
        terminal_response,
    );
    atomic_create(&dir, &filename, body.as_bytes())?;
    mark_returned(workspace, child_id, terminal_ref, git)?;
    Ok(dir.join(filename))
}

/// Write the durable returned mark `refs/litany/returned/<child-id>` at
/// the child's terminal ref (module docs on [`RETURNED_REF_PREFIX`]).
fn mark_returned(
    workspace: &Path,
    child_id: &str,
    terminal_ref: &str,
    git: &dyn GitRunner,
) -> Result<(), DepositError> {
    git.run(
        &crate::workspace::repo_git(workspace),
        &["update-ref", &returned_ref(child_id), terminal_ref],
    )
    .map_err(|source| DepositError::Mark {
        child: child_id.to_string(),
        source,
    })
}

/// Render a result message (§2.6, §2.11): the ordinary `from:` /
/// `deposited_at:` frontmatter plus `epitaph:` and `terminal_ref:`, then
/// the terminal response as the body — present iff `Some`. When the
/// agent never spoke the file ends at the closing frontmatter delimiter
/// with no body, which is exactly how delivery composes an empty
/// user-role wire message for it.
fn render_result(
    child_id: &str,
    deposited_at: &str,
    epitaph: Epitaph,
    terminal_ref: &str,
    terminal_response: Option<&str>,
) -> String {
    let head = format!(
        "---\nfrom: {child_id}\ndeposited_at: {deposited_at}\n\
         epitaph: {ep}\nterminal_ref: {terminal_ref}\n---\n",
        ep = epitaph.as_str(),
    );
    match terminal_response {
        Some(body) => format!("{head}{body}"),
        None => head,
    }
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
