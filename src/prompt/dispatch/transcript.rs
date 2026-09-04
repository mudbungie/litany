//! Committing transcript entries to the branch (ARCH §2.3, §3.3).
//!
//! The **transcript** is the branch-scoped sequence under `messages/`:
//! each step's model output and each tool call's result is one
//! immutable entry, committed by the executor as it lands (§2.3). An
//! entry's filename is `NNN-<origin>.json` — `NNN` the branch's single
//! zero-padded transcript counter and `<origin>` the entry's author: the
//! **model id** that produced a model-output entry (§2.3, §4.3), or the
//! one reserved token `tool` for a tool result. Order lives in the
//! filename and nowhere else, so the counter is *derived* — [`next_seq`]
//! reads the `messages/` listing and returns max-present-plus-one —
//! never stored (PRINCIPLES single source of truth).
//!
//! Each entry file is a JSON array of brazen's canonical [`Content`]
//! blocks (a model-output entry's streamed blocks; a tool entry's single
//! `tool_result` block), so it composes verbatim as one wire message
//! (§2.3) — what makes replay bit-identical rather than a lossy
//! re-rendering.

use super::entry;
use crate::prompt::Error;
use crate::template::GitRunner;
use brazen::Content;
use std::path::Path;

/// Branch-scoped transcript directory (ARCH §2.3 — `messages/NNN-…`).
pub(crate) const MESSAGES_DIR: &str = "messages";
/// Zero-pad width of the transcript counter, matching the step-record
/// convention (`steps/<id>/NNN`, `summary/NNN`).
const SEQ_WIDTH: usize = 3;
/// The one reserved `.json` origin token (§2.3): a tool call's result.
/// A model-output entry's origin token is instead the model id that
/// authored it, so a model id colliding with this token is declined
/// ([`commit_assistant`]), never munged.
const TOOL_ORIGIN: &str = "tool";

/// The branch's next transcript counter: max of the `NNN` prefixes
/// present under `<worktree>/messages/`, plus one (§2.3). An absent or
/// empty directory yields `1` — the general path with empty inputs, not
/// a bootstrap special case.
pub(super) fn next_seq(worktree: &Path) -> Result<u32, Error> {
    let dir = worktree.join(MESSAGES_DIR);
    let entries = match std::fs::read_dir(&dir) {
        Ok(rd) => rd,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(1),
        Err(e) => return Err(Error::Io(e)),
    };
    let mut max = 0u32;
    for entry in entries {
        let name = entry.map_err(Error::Io)?.file_name();
        if let Some(seq) = name
            .to_string_lossy()
            .split('-')
            .next()
            .and_then(|p| p.parse::<u32>().ok())
        {
            max = max.max(seq);
        }
    }
    Ok(max + 1)
}

/// Deliver a deposited inbox message into the transcript (§2.11
/// *Delivery*): the message file at `src` *moves* by a literal
/// `rename(2)` into `messages/NNN-<sender>.md` at the branch's next
/// counter (§2.3 *Origins*), then a commit lands it (the delivery
/// commit). The message file has exactly one home at every instant — it
/// left the inbox and lands in the transcript in one atomic move.
/// `sender` is the depositing sender parsed from the inbox filename (the
/// path carries framing; the file's frontmatter travels untouched, §2.11)
/// and becomes the origin token; the entry composes as user-role content
/// (§5.3).
pub(super) fn deliver_message(
    worktree: &Path,
    conv_id: &str,
    sender: &str,
    src: &Path,
    git: &dyn GitRunner,
) -> Result<(), Error> {
    let seq = next_seq(worktree)?;
    let rel = format!("{MESSAGES_DIR}/{seq:0w$}-{sender}.md", w = SEQ_WIDTH);
    let dest = worktree.join(&rel);
    std::fs::create_dir_all(dest.parent().expect("messages/ has a parent"))?;
    std::fs::rename(src, &dest)?;
    commit_entry(worktree, conv_id, seq, &[&rel], sender, git)
}

/// Commit a step's model output: seal-and-rename — the sealed staging
/// file (§2.3 *The transcript writer*) *leaves* by rename into
/// `messages/NNN-<model-id>.json` at the branch's next counter, then a
/// commit lands it. The origin token is `model_id` — the model that
/// authored the entry, as it rode the canonical request (§2.3, §4.3); a
/// model id colliding with the one reserved `.json` token [`TOOL_ORIGIN`]
/// is declined (decline illegal operations, §2.3), never munged. `NNN` is
/// evaluated here, inside the executor's serialized commit section (§2.3).
/// Returns the committed canonical blocks — read back from the transcript
/// entry (its one content home, §2.3), never from any `steps/` record —
/// so the step loop can run this step's `tool_use` calls without a second
/// content fold.
pub(super) fn commit_assistant(
    worktree: &Path,
    conv_id: &str,
    model_id: &str,
    staging_path: &Path,
    git: &dyn GitRunner,
) -> Result<Vec<Content>, Error> {
    if model_id == TOOL_ORIGIN {
        return Err(Error::ReservedModelId(model_id.to_string()));
    }
    let seq = next_seq(worktree)?;
    let rel = entry_rel(seq, model_id);
    let dest = worktree.join(&rel);
    std::fs::create_dir_all(dest.parent().expect("messages/ has a parent"))?;
    std::fs::rename(staging_path, &dest)?;
    commit_entry(worktree, conv_id, seq, &[&rel], model_id, git)?;
    let bytes = std::fs::read(&dest)?;
    Ok(entry::blocks(&bytes))
}

/// Commit one resolved tool call's canonical `tool_result` block as
/// `messages/NNN-tool.json` (§3.3 "Wire `tool_result` framing is
/// transcript-backed") **together with any worktree side effects the
/// tool produced** — a copied `skills/<name>/` body (§3.3 Body-on-demand,
/// [`crate::prompt::tool::builtin::load_skill`]), a file a shell tool
/// wrote, etc. §2.3 pins this: "each tool call the step emitted commits
/// its result — and any worktree side effects — as it lands." So this
/// entry stages the whole worktree (`git add -A`), not just the result
/// file: the `steps/` and `inbox/` trees sit at the workspace root
/// outside every worktree (§2.2), so `-A` captures exactly the tool's
/// worktree footprint and nothing diagnostic. A read-only tool touches
/// nothing but its result entry, so `-A` degenerates to the single-file
/// stage. The counter read happens inside the sibling tool serialization
/// the caller already imposes (§3.3).
pub(super) fn commit_tool(
    worktree: &Path,
    conv_id: &str,
    tool_result: &Content,
    git: &dyn GitRunner,
) -> Result<(), Error> {
    let seq = next_seq(worktree)?;
    let rel = entry_rel(seq, TOOL_ORIGIN);
    let dest = worktree.join(&rel);
    std::fs::create_dir_all(dest.parent().expect("messages/ has a parent"))?;
    let bytes = serde_json::to_vec(std::slice::from_ref(tool_result)).expect("Content serializes");
    std::fs::write(&dest, bytes)?;
    commit_entry(worktree, conv_id, seq, &["-A"], TOOL_ORIGIN, git)
}

/// The `tool_use` ids that already have a committed `tool_result` entry
/// — every `messages/NNN-tool.json`'s block ids (§2.3). Read by the tool
/// window only when a hold mark is in play (§3.3 *Tool control*): a
/// resumed window re-runs its step's blocks and must skip the ones whose
/// results landed before the park, without a stored cursor — the
/// transcript is the record (PRINCIPLES, single source of truth). An
/// absent `messages/` yields the empty set.
pub(super) fn committed_result_ids(
    worktree: &Path,
) -> Result<std::collections::HashSet<String>, Error> {
    let mut ids = std::collections::HashSet::new();
    for bytes in tool_entries(worktree)? {
        for block in entry::blocks(&bytes) {
            if let Content::ToolResult { tool_use_id, .. } = block {
                ids.insert(tool_use_id);
            }
        }
    }
    Ok(ids)
}

/// The committed bytes of every `messages/NNN-tool.json` entry — the
/// tool half of the transcript, read out of the read-state tree (§2.3).
/// Two folds run over it and neither owns the walk: the answered-ids
/// read above, and the context-file *shown* query
/// ([`super::tool_step`], §3.3), which asks whether any committed entry
/// already frames a path. An absent `messages/` yields the empty
/// sequence — the general path with empty inputs.
pub(super) fn tool_entries(worktree: &Path) -> Result<Vec<Vec<u8>>, Error> {
    let dir = worktree.join(MESSAGES_DIR);
    let rd = match std::fs::read_dir(&dir) {
        Ok(rd) => rd,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(Error::Io(e)),
    };
    let mut entries = Vec::new();
    for entry in rd {
        let path = entry.map_err(Error::Io)?.path();
        let name = path
            .file_name()
            .map(|n| n.to_string_lossy())
            .unwrap_or_default();
        if !name.ends_with(&format!("-{TOOL_ORIGIN}.json")) {
            continue;
        }
        entries.push(std::fs::read(&path)?);
    }
    Ok(entries)
}

/// `messages/NNN-<origin>.json` for `seq`, zero-padded to [`SEQ_WIDTH`].
fn entry_rel(seq: u32, origin: &str) -> String {
    format!("{MESSAGES_DIR}/{seq:0w$}-{origin}.json", w = SEQ_WIDTH)
}

/// `git add <add_args>` then commit the entry on the conversation
/// branch. `add_args` is the pathspec to stage: a single `messages/…`
/// entry for a delivery or model-output commit (their only footprint is
/// that one file), or `-A` for a tool commit (which additionally captures
/// the tool's worktree side effects, per [`commit_tool`]).
fn commit_entry(
    worktree: &Path,
    conv_id: &str,
    seq: u32,
    add_args: &[&str],
    origin: &str,
    git: &dyn GitRunner,
) -> Result<(), Error> {
    let mut argv = vec!["add"];
    argv.extend_from_slice(add_args);
    git.run(worktree, &argv).map_err(|source| Error::Git {
        op: "transcript add",
        source,
    })?;
    let msg = format!("transcript {seq:0w$}: {origin} [{conv_id}]", w = SEQ_WIDTH);
    git.run(worktree, &["commit", "-m", msg.as_str()])
        .map_err(|source| Error::Git {
            op: "transcript commit",
            source,
        })
}

#[cfg(test)]
mod tests;
