//! Step-boundary inbox drain (ARCH §2.11 *Delivery*).
//!
//! At each step boundary — after the prior step's tool entries commit,
//! before the next model call's context assembles (§2.3 drain ordering) —
//! the executor drains the agent's inbox. [`drain`] first commits any
//! renamed-but-uncommitted stray a prior death left in `messages/` (the
//! file left the inbox in a rename that never reached its commit, so it
//! must land — the inverse of §2.3's "an entry that never committed never
//! happened"), then moves each pending inbox message into
//! `messages/NNN-<sender>.md` and commits it (the delivery commit,
//! [`transcript::deliver_message`]).
//!
//! Arrival order across senders is advisory (mtimes); the committed
//! sequence is the record (§2.11). The drain order is deterministic —
//! `(mtime, filename)` — so the same inbox always yields the same
//! committed sequence and replay agrees with the live run.

use super::transcript;
use crate::prompt::Error;
use crate::prompt::inbox::USER_SENDER;
use crate::template::GitRunner;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

/// Extension of a deposited inbox message file (§2.11 *Deposit* —
/// `<sender>-<NNN>.md`).
const MESSAGE_EXT: &str = "md";

/// Drain `inbox` into the branch `worktree` at a step boundary (§2.11).
/// The whole drain runs inside the executor lock the step loop already
/// holds, so [`transcript::next_seq`]'s max-present-plus-one counter is
/// race-free (§2.3).
///
/// Returns what this drain did — delivery count, plus the identities of
/// the deposits it deliberately left pending ([`Delivery::left`]). That
/// left-set is the §2.11 release rule's diff base, this executor's last
/// *accounted* inbox read under the lease
/// ([`super::driver::release_then_reprobe`]): a pending deposit outside
/// it raced the lease and warrants the releaser's launch.
pub(super) fn drain(
    worktree: &Path,
    inbox: &Path,
    conv_id: &str,
    git: &dyn GitRunner,
) -> Result<Delivery, Error> {
    recover_strays(worktree, conv_id, git)?;
    let mut delivery = Delivery {
        delivered: 0,
        operator_spoke: false,
        left: Vec::new(),
    };
    for msg in pending(inbox)? {
        // A **result message from this agent's own child** (§2.6,
        // carrying a `terminal_ref:`) is a lifecycle circumstance the §6
        // hop interprets by the returning child's role — deliver_result,
        // land_compaction, or a gate-hold (`super::child_result`) — not an
        // ordinary steering message. The drain leaves it in the inbox for
        // that interpreter and delivers everything else here, a sibling's
        // reply included (`child_result::own_result_ref` carries the
        // reasoning; the hold is a disk query over the inbox,
        // `docs/PRINCIPLES.md` Single source of truth).
        let body = std::fs::read_to_string(&msg.path).map_err(Error::Io)?;
        if super::child_result::own_result_ref(conv_id, &msg.sender, &body).is_some() {
            delivery.left.push(SeenDeposit {
                name: msg.name,
                mtime: msg.mtime,
            });
            continue;
        }
        transcript::deliver_message(worktree, conv_id, &msg.sender, &msg.path, git)?;
        delivery.delivered += 1;
        delivery.operator_spoke |= msg.sender == USER_SENDER;
    }
    Ok(delivery)
}

/// What one [`drain`] did: how many messages moved as delivery commits,
/// and which deposits it deliberately left pending.
#[derive(Debug)]
pub(super) struct Delivery {
    /// Messages moved into the transcript as delivery commits (§2.11).
    pub(super) delivered: usize,
    /// At least one of them was the operator's ([`USER_SENDER`], §2.3
    /// *Origins*) — the circumstance that fires `user_message` (§6,
    /// `docs/DESIGN_PROCEDURES.md` §3). A circumstance, not a count: the
    /// event fires once per hop that delivered any.
    pub(super) operator_spoke: bool,
    /// The §2.11 release rule's seen-set: deposits this drain enumerated
    /// and deliberately left (held results, `super::child_result`).
    pub(super) left: Vec<SeenDeposit>,
}

/// The identity of a deposit a drain deliberately left pending — one
/// element of the §2.11 release rule's seen-set. Identity is the
/// `(name, mtime)` pair, never the name alone: delivery and result
/// interpretation free a `<sender>-<NNN>` name for reuse (§2.11
/// *Deposit* derives `NNN` from the current listing), and a reused name
/// is a *new* deposit that must fire the rule — while a held file is
/// untouched from enumeration to release, so both fields are stable
/// exactly as long as the file is the one the drain saw.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct SeenDeposit {
    name: String,
    mtime: SystemTime,
}

impl SeenDeposit {
    /// Construct from parts — the test seam for exercising the release
    /// rule's arms without a full drain ([`super::driver`] tests).
    #[cfg(test)]
    pub(super) fn new(name: String, mtime: SystemTime) -> Self {
        SeenDeposit { name, mtime }
    }

    /// Whether `pending` is the very deposit this identity names — same
    /// name *and* same mtime (a reused name carries a fresh mtime).
    pub(super) fn matches(&self, pending: &Pending) -> bool {
        self.name == pending.name && self.mtime == pending.mtime
    }
}

/// Enumerate the inbox and account **everything** as deliberately left —
/// the seen-set of a hop that delivers nothing by design: the held-branch
/// entry (§3.3 *Tool control*), where delivering mail mid-tool-window
/// would wedge a user entry between a `tool_use` and its eventual
/// `tool_result` (§2.3 pairing). Every pending deposit is accounted, so
/// the §2.11 release rule launches nothing for it — a parked branch
/// queues its mail rather than relaunch-looping, and the next drive
/// after release delivers it at the ordinary step boundary.
pub(super) fn seen_all(inbox: &Path) -> Result<Vec<SeenDeposit>, Error> {
    Ok(pending(inbox)?
        .into_iter()
        .map(|m| SeenDeposit {
            name: m.name,
            mtime: m.mtime,
        })
        .collect())
}

/// One deliverable inbox message, carrying the sort keys `(mtime, name)`
/// (§2.11 — advisory arrival order made a deterministic committed
/// sequence).
#[derive(Debug)]
pub(super) struct Pending {
    /// Advisory arrival-order key (§2.11) — and one half of the
    /// [`SeenDeposit`] identity pair.
    mtime: SystemTime,
    /// The deposit filename (`<sender>-<NNN>.md`) — the other half of
    /// the [`SeenDeposit`] identity pair.
    pub(super) name: String,
    /// Absolute path of the inbox file — read by the §6 child-result
    /// interpreter ([`super::child_result`]) to route a result message.
    pub(super) path: PathBuf,
    /// The `<sender>` (a child's agent id, for a result message) — the
    /// interpreter's key for role derivation and delivery.
    pub(super) sender: String,
}

/// List the inbox's deliverable messages, sorted `(mtime, filename)`. An
/// absent inbox yields nothing — the general path with empty inputs, not
/// a bootstrap special case. A name that is not a well-formed
/// `<sender>-<NNN>.md` deposit (an in-flight `.tmp`, a stray) contributes
/// nothing: only real deposits are delivered. `pub(super)` for the
/// launched driver's found-anything test ([`super::driver`]).
pub(super) fn pending(inbox: &Path) -> Result<Vec<Pending>, Error> {
    let rd = match std::fs::read_dir(inbox) {
        Ok(rd) => rd,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(Error::Io(e)),
    };
    let mut out = Vec::new();
    for entry in rd {
        let entry = entry.map_err(Error::Io)?;
        let name = entry.file_name().to_string_lossy().into_owned();
        let Some(sender) = sender_of(&name) else {
            continue;
        };
        // mtime is advisory — the committed counter is the record
        // (§2.11). An unreadable one sorts to the epoch (delivered first);
        // any genuine trouble with the file resurfaces at its rename.
        let mtime = entry
            .metadata()
            .and_then(|m| m.modified())
            .unwrap_or(SystemTime::UNIX_EPOCH);
        out.push(Pending {
            mtime,
            name,
            path: entry.path(),
            sender,
        });
    }
    out.sort_by(|a, b| (a.mtime, &a.name).cmp(&(b.mtime, &b.name)));
    Ok(out)
}

/// The `<sender>` of a `<sender>-<NNN>.md` deposit name (§2.11 *Deposit*):
/// strip the `.md` suffix, split off the trailing `-<NNN>` numeric
/// sequence, and keep the remainder — so `user-001.md` → `user` and a
/// hyphenated agent id `a-b-c-002.md` → `a-b-c`. A name with no numeric
/// tail, the wrong extension, an empty sender, or a leading-dot temp
/// prefix yields `None`.
fn sender_of(name: &str) -> Option<String> {
    let stem = name.strip_suffix(&format!(".{MESSAGE_EXT}"))?;
    let (sender, seq) = stem.rsplit_once('-')?;
    seq.parse::<u32>().ok()?;
    (!sender.is_empty() && !sender.starts_with('.')).then(|| sender.to_string())
}

/// Commit any renamed-but-uncommitted stray a prior death left under
/// `messages/` (§2.11): a delivery whose `rename(2)` landed but whose
/// commit never ran leaves the file untracked in the worktree, and it
/// must land. Scoped to `messages/` so a worktree left dirty elsewhere (a
/// tool side effect mid-step) is never swept into the delivery commit. A
/// clean `messages/` — the common case — reports nothing and this is a
/// no-op.
fn recover_strays(worktree: &Path, conv_id: &str, git: &dyn GitRunner) -> Result<(), Error> {
    let status = git
        .run_capture(
            worktree,
            &["status", "--porcelain", "--", transcript::MESSAGES_DIR],
        )
        .map_err(|source| Error::Git {
            op: "drain status",
            source,
        })?;
    if status.trim().is_empty() {
        return Ok(());
    }
    git.run(worktree, &["add", transcript::MESSAGES_DIR])
        .map_err(|source| Error::Git {
            op: "drain recover add",
            source,
        })?;
    let msg = format!("transcript: recover delivered stray [{conv_id}]");
    git.run(worktree, &["commit", "-m", msg.as_str()])
        .map_err(|source| Error::Git {
            op: "drain recover commit",
            source,
        })
}

#[cfg(test)]
mod tests;
