//! Prune the inherited **unsettled tool step** from the forked tree, at
//! the dispatch commit (ARCH §2.3 step 2, §2.5, §3.3, §5.4).
//!
//! A tool-call dispatch runs *inside* the parent's tool step (§2.5): the
//! parent's model-output entry — carrying the `tool_use` block whose
//! execution is the dispatch — is already committed, and the answering
//! `messages/NNN-tool.json` cannot commit until the tool returns, which
//! is after the fork. So the child forks off a tip whose transcript ends
//! in what §3.3 already names *tool in progress*: a `tool_use` block with
//! no matching committed `tool_result` entry.
//!
//! That tail is legal on the parent's branch — it settles moments later —
//! and illegal on the child's, where nothing will ever answer it. Every
//! provider validates each `tool_use` against a `tool_result` in the
//! immediately following wire message (§2.5), so the child's first model
//! call was refused outright: `{"kind":{"provider":{"status":400}},
//! "message":"No tool output found for function call call_…"}` (bl-4231),
//! which killed the whole agent-initiated dispatch path.
//!
//! **The child's record is made honest at fork time, not at read time.**
//! The dispatch commit deletes the unsettled tail — the trailing
//! model-output entry whose `tool_use` blocks are not all answered, and
//! every entry after it (its partial results, which orphan the moment
//! their `tool_use` leaves). Deletion is the sanctioned transcript change
//! (§2.3 *change is append or delete, never edit-in-place*; §5.4) and the
//! parent's own copy is untouched, so nothing is lost: the entry stays on
//! the parent's branch and in the child's git history.
//!
//! The alternative — teaching assembly to skip a trailing unpaired
//! `tool_use` — is rejected on §2.7's own reasoning about the compactor's
//! inherited blocks: the wire framing is transcript-backed, so a filter
//! there would make the model call disagree with the branch's record, and
//! assembly would stop being a pure function of the tree (§5.1). Moving
//! the fork point past the tool step is rejected too: the dispatch's
//! `tool_result` *is* the child's address (§2.5), so the settled boundary
//! does not exist until after the fork must have happened.
//!
//! **That rejection is about the call side and stands** (bl-2d93). The
//! read-side drop assembly *does* perform — [`super::super::pairing::
//! drop_orphans`], a `tool_result` whose `tool_use` is absent — is the
//! opposite case in every respect that mattered here: the write side has
//! no repair for it (the call is already gone from the tree, so nothing
//! a cut can do brings it back), the branch is otherwise dead at every
//! later prompt rather than at one, and the drop is *recorded* in the
//! step record rather than being a silent disagreement with the record.
//! An unanswered `tool_use` still leaves by deletion, here, at the fork.
//!
//! It lives in [`super::trim_to_context`] beside the control-file and
//! descriptor removals because it is the same act: the dispatch commit
//! trimming the forked tree to exactly what this agent may hold. Total
//! and idempotent like its siblings — a root forked off a config commit
//! carries no transcript, a compactor forks off a checkpoint commit read
//! at a *closed* tool step (§2.7), and a verifier forks off a terminal
//! ref; all three settle to no git command at all.

use crate::prompt::Error;
use crate::prompt::dispatch::{MESSAGES_DIR, entry, pairing};
use crate::template::GitRunner;
use brazen::Content;
use std::path::Path;

/// Stage the removal of the inherited transcript's unsettled tool step.
///
/// Issues **no** git command when the inherited tail is settled — every
/// fork point but a mid-tool-step one, and every re-run over an
/// already-pruned tree.
pub(crate) fn prune_unsettled(worktree: &Path, git: &dyn GitRunner) -> Result<(), Error> {
    let entries = sequence(worktree)?;
    let Some(cut) = unsettled_from(worktree, &entries)? else {
        return Ok(());
    };
    let mut args: Vec<&str> = vec!["rm", "-q", "--"];
    args.extend(entries[cut..].iter().map(String::as_str));
    git.run(worktree, &args).map_err(|source| Error::Git {
        op: "rm unsettled tool step",
        source,
    })
}

/// The branch's transcript entries as worktree-relative paths, ordered by
/// the filename's `NNN` counter — order lives in the name and nowhere
/// else (§2.3). An absent `messages/` yields none: the general path with
/// empty inputs, which is every fresh root.
fn sequence(worktree: &Path) -> Result<Vec<String>, Error> {
    let dir = worktree.join(MESSAGES_DIR);
    let read = match std::fs::read_dir(&dir) {
        Ok(rd) => rd,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(Error::Io(e)),
    };
    let mut names: Vec<String> = Vec::new();
    for entry in read {
        let name = entry.map_err(Error::Io)?.file_name();
        names.push(format!("{MESSAGES_DIR}/{}", name.to_string_lossy()));
    }
    Ok(pairing::ordered(names))
}

/// Where the inherited transcript's trailing **unsettled window**
/// begins, or `None` when the tail is settled — the pairing invariant's
/// one derivation ([`pairing::unsettled_from`]), read here over the
/// forked worktree's own files. The fork and the compaction landing ask
/// the same question of two different trees, so neither owns the answer.
fn unsettled_from(worktree: &Path, entries: &[String]) -> Result<Option<usize>, Error> {
    pairing::unsettled_from(entries, &|rel: &str| blocks(worktree, rel))
}

/// The canonical blocks of one `.json` transcript entry, through the
/// entry shape's one home (§2.3, [`entry`]).
fn blocks(worktree: &Path, rel: &str) -> Result<Vec<Content>, Error> {
    let bytes = std::fs::read(worktree.join(rel)).map_err(Error::Io)?;
    Ok(entry::blocks(&bytes))
}

#[cfg(test)]
mod tests;
