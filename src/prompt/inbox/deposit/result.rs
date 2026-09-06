//! The **result message** (ARCH §2.6, §2.11): the deposit a returning
//! agent's executor makes on its behalf, and the durable mark that
//! outlives it.
//!
//! It is an ordinary [`super::deposit`] in every respect but two — its
//! frontmatter carries the two pinned fields `epitaph:` and
//! `terminal_ref:`, and it writes the returned mark — so it is built from
//! [`super::fields`] rather than beside it. Split from its parent at
//! bl-a457, when the sender-name read pushed the file to the per-file
//! cap: the ordinary deposit's shape and the terminal event's pinned
//! facts change for different reasons.

use super::{DepositError, atomic_create, fields, io_err, message_filename, next_sequence};
use crate::prompt::Clock;
use crate::template::GitRunner;

use std::path::{Path, PathBuf};

use super::super::inbox_dir;

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
    let fields = fields(workspace, child_id, &clock.now_iso8601(), git);
    let body = render_result(&fields, epitaph, terminal_ref, terminal_response);
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

/// Render a result message (§2.6, §2.11): the ordinary [`fields`] plus
/// `epitaph:` and `terminal_ref:` in the same frontmatter block, then the
/// terminal response as the body — present iff `Some`. When the agent
/// never spoke the file ends at the closing delimiter with no body, which
/// is exactly how delivery composes an empty user-role wire message for
/// it.
fn render_result(
    fields: &str,
    epitaph: Epitaph,
    terminal_ref: &str,
    terminal_response: Option<&str>,
) -> String {
    let head = format!(
        "---\n{fields}epitaph: {ep}\nterminal_ref: {terminal_ref}\n---\n",
        ep = epitaph.as_str(),
    );
    match terminal_response {
        Some(body) => format!("{head}{body}"),
        None => head,
    }
}
