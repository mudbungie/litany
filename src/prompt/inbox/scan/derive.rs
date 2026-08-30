//! The workspace scan's derivations (ARCH §2.11 *Crashes are a failure
//! class*, §8 — the `litany scan` operator verb).
//!
//! Pure and read-only over the workspace: branch enumeration, the
//! live-executor probe, the returned/never-deposited derivation across a
//! parent's inbox and transcript, the died-mid-work classification over
//! `steps/`, and the inbox-flush enumeration. The orchestration that acts
//! on these lives in [`super`]; keeping the derivations here holds both
//! files under the repo's per-file line cap and makes every predicate
//! unit-testable in isolation.

use super::ScanError;
use crate::prompt::inbox::{INBOX_DIR, inbox_dir, try_acquire};
use crate::prompt::step::latest_step_outcome;
use crate::provider::segment::Outcome;
use crate::template::GitRunner;
use crate::workspace;
use std::io;
use std::path::Path;

/// The transcript directory a delivered message lands in, as
/// `messages/<NNN>-<sender>.md` (§2.11 *Delivery*). Read via `git
/// ls-tree` off the parent branch to detect a *delivered* return.
const MESSAGES_DIR: &str = "messages";
/// Message-file extension (§2.11 *Deposit* — `<sender>-<NNN>.md`).
const MESSAGE_EXT: &str = "md";

/// The candidate agent set (§8): every `agents/*` ref, prefix stripped
/// to the agent id. This is the enumeration seam §8 names — the prefix
/// is the kind (§2.3), so config branches are excluded structurally,
/// not by subtracting a reserved name.
pub(super) fn agent_branches(
    workspace: &Path,
    git: &dyn GitRunner,
) -> Result<Vec<String>, ScanError> {
    workspace::agent_ids(workspace, git).map_err(|source| ScanError::Git {
        op: "for-each-ref agents/",
        source,
    })
}

/// Is `branch` currently driven? A non-blocking [`try_acquire`] whose
/// *success* means nobody holds the lock (the lease is dropped at once —
/// probing is not driving). `Ok(true)` ⇒ another executor owns it.
pub(super) fn is_driven(workspace: &Path, branch: &str) -> Result<bool, ScanError> {
    let dir = inbox_dir(workspace, branch);
    match try_acquire(&dir).map_err(|source| ScanError::Probe {
        path: dir.clone(),
        source,
    })? {
        Some(_guard) => Ok(false),
        None => Ok(true),
    }
}

/// Has the child `child` returned a result to `parent`? The durable
/// answer is the **returned mark** `refs/litany/returned/<child>` that
/// every result deposit writes ([`crate::prompt::inbox::deposit::RETURNED_REF_PREFIX`])
/// — the message file and even its delivered transcript entry are
/// consumable (a compaction landing removes the trigger without a
/// transcript entry, and a later compaction can squash a delivered
/// `messages/NNN-<child>.md` away), so their absence proves nothing.
/// The two legacy reads — a message from the child in the parent's inbox
/// (undelivered) or in its transcript (delivered) — remain as the
/// deposit-crash-window belt and for workspaces predating the mark. Any
/// presence means the return already happened, so the sweep must not
/// deposit again.
pub(super) fn returned(
    workspace: &Path,
    git: &dyn GitRunner,
    parent: &str,
    child: &str,
) -> Result<bool, ScanError> {
    if returned_mark_exists(workspace, git, child) || has_inbox_message(workspace, parent, child) {
        return Ok(true);
    }
    transcript_has(workspace, git, parent, child)
}

/// Does the durable returned mark exist for `child`? A `show-ref
/// --verify --quiet` probe against the bare repo.git; any failure —
/// including an absent ref's exit 1 — reads as "no mark", falling
/// through to the legacy evidence reads.
fn returned_mark_exists(workspace: &Path, git: &dyn GitRunner, child: &str) -> bool {
    let mark = crate::prompt::inbox::deposit::returned_ref(child);
    git.run(
        &workspace::repo_git(workspace),
        &["show-ref", "--verify", "--quiet", &mark],
    )
    .is_ok()
}

/// Is there an undelivered message *from* `child` in `parent`'s inbox — a
/// file named `<child>-<NNN>.md`? An absent or unreadable inbox is "no
/// message" (the general path with empty inputs).
fn has_inbox_message(workspace: &Path, parent: &str, child: &str) -> bool {
    let dir = inbox_dir(workspace, parent);
    let Ok(rd) = std::fs::read_dir(&dir) else {
        return false;
    };
    rd.flatten()
        .any(|e| is_message_from(&e.file_name().to_string_lossy(), child))
}

/// Does `name` name a deposit from `sender` — `<sender>-<NNN>.md` with a
/// numeric `<NNN>`? The same shape [`crate::prompt::inbox::deposit`]
/// writes.
pub(super) fn is_message_from(name: &str, sender: &str) -> bool {
    let prefix = format!("{sender}-");
    let suffix = format!(".{MESSAGE_EXT}");
    name.strip_prefix(&prefix)
        .and_then(|m| m.strip_suffix(&suffix))
        .is_some_and(|seq| seq.parse::<u32>().is_ok())
}

/// Has a message from `child` been *delivered* into `parent`'s transcript
/// — a committed `messages/<NNN>-<child>.md` on the parent branch? Read
/// with `git ls-tree` off the `agents/<parent>` ref against the bare
/// repo.git, so a torn-down worktree is no obstacle (§2.3 step 6).
fn transcript_has(
    ws: &Path,
    git: &dyn GitRunner,
    parent: &str,
    child: &str,
) -> Result<bool, ScanError> {
    let parent_ref = workspace::agent_ref(parent);
    let out = git
        .run_capture(
            &workspace::repo_git(ws),
            &[
                "ls-tree",
                "-r",
                "--name-only",
                parent_ref.as_str(),
                "--",
                MESSAGES_DIR,
            ],
        )
        .map_err(|source| ScanError::Git {
            op: "ls-tree messages",
            source,
        })?;
    Ok(out.lines().any(|line| transcript_line_from(line, child)))
}

/// Does a `messages/<NNN>-<sender>.md` transcript path name a delivery
/// from `sender`? Strip the dir and `.md`, split the leading numeric
/// `<NNN>-` off, and compare the remainder (`007-a-b` → sender `a-b`).
pub(super) fn transcript_line_from(line: &str, sender: &str) -> bool {
    let file = line.rsplit('/').next().unwrap_or(line);
    let Some(stem) = file.strip_suffix(&format!(".{MESSAGE_EXT}")) else {
        return false;
    };
    match stem.split_once('-') {
        Some((seq, rest)) => seq.parse::<u32>().is_ok() && rest == sender,
        None => false,
    }
}

/// Did `branch` die mid-work — did its latest step's model call never
/// settle complete (§2.3)? Two on-disk shapes say so, and both are dead:
/// `response.json` closed without a terminal `end` (killed or stopped
/// mid-stream, §2.9, [`Outcome::NoTerminal`]) or its last segment
/// terminated in an `Error` (retries exhausted or a non-retryable error,
/// §2.10, [`Outcome::Failed`]) — either way no transcript entry
/// committed and the branch cannot advance without a new touch. The one
/// derivation is [`latest_step_outcome`] (a §2.3-sanctioned framing
/// read); no `steps/` tree (the shipped child shape) or no readable
/// response ⇒ this signal is silent (`None` ⇒ `false`).
pub(super) fn died_mid_work(workspace: &Path, branch: &str) -> bool {
    matches!(
        latest_step_outcome(workspace, branch),
        Some(Outcome::NoTerminal | Outcome::Failed)
    )
}

/// The branch tip sha — the child's `terminal_ref:` for the sweep's
/// `died` deposit (§2.6). `git rev-parse --verify agents/<id>` against
/// the bare repo.git.
pub(super) fn branch_tip(
    ws: &Path,
    git: &dyn GitRunner,
    branch: &str,
) -> Result<String, ScanError> {
    let branch_ref = workspace::agent_ref(branch);
    let out = git
        .run_capture(
            &workspace::repo_git(ws),
            &["rev-parse", "--verify", branch_ref.as_str()],
        )
        .map_err(|source| ScanError::Git {
            op: "rev-parse branch tip",
            source,
        })?;
    Ok(out.trim().to_string())
}

/// Every agent that has an inbox directory under `<workspace>/inbox/`,
/// sorted. An absent inbox root yields nothing (the general path with
/// empty inputs, not a bootstrap case).
pub(super) fn inbox_agents(workspace: &Path) -> Result<Vec<String>, ScanError> {
    let root = workspace.join(INBOX_DIR);
    let rd = match std::fs::read_dir(&root) {
        Ok(rd) => rd,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(source) => return Err(ScanError::InboxRoot { path: root, source }),
    };
    let mut agents: Vec<String> = rd
        .flatten()
        .filter(|e| e.file_type().map(|t| t.is_dir()).unwrap_or(false))
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .collect();
    agents.sort();
    Ok(agents)
}

/// Does this inbox hold at least one well-formed pending deposit
/// (`<sender>-<NNN>.md`)? A leading-dot temp file or a stray is not a
/// deposit and does not count.
pub(super) fn has_pending(dir: &Path) -> bool {
    let Ok(rd) = std::fs::read_dir(dir) else {
        return false;
    };
    rd.flatten().any(|e| {
        let name = e.file_name().to_string_lossy().into_owned();
        is_pending_deposit(&name)
    })
}

/// Is `name` a well-formed `<sender>-<NNN>.md` deposit? Splits the numeric
/// `<NNN>` tail off the `.md` stem; a non-numeric tail, wrong extension,
/// empty sender, or a `.tmp`/leading-dot temp is not a deposit.
pub(super) fn is_pending_deposit(name: &str) -> bool {
    let Some(stem) = name.strip_suffix(&format!(".{MESSAGE_EXT}")) else {
        return false;
    };
    match stem.rsplit_once('-') {
        Some((sender, seq)) => {
            seq.parse::<u32>().is_ok() && !sender.is_empty() && !sender.starts_with('.')
        }
        None => false,
    }
}
