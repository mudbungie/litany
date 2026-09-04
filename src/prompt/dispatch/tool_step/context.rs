//! **Context files** on the tool result (ARCH §3.3 *Context files ride
//! the next tool result*, `docs/DESIGN_CONTEXT_ECONOMY.md` §6).
//!
//! A context file is an `AGENTS.md` or a `CLAUDE.md` — whatever the
//! governing workflow's `context_files:` list names — sitting on the
//! path from the enclosing repository's top level down to the agent's
//! working directory. Every result a tool call produces carries, after
//! its envelope, each such file this agent has not been shown yet.
//!
//! **Not a `cd` side channel — a query at every tool result.** The fact
//! is the cwd, and the cwd has two writers: the agent's own `cd` and
//! the `--cwd` seed at creation (§3.3), which has no tool result of its
//! own to append to. Asking at every result dissolves both: `cd` is
//! simply the tool whose result usually comes next, and a seeded agent's
//! *first* result carries its seed directory's files.
//!
//! **Shown once per agent, derived.** "Already shown" is a query over
//! the committed transcript — does any tool entry frame that path
//! ([`crate::prompt::tool::frame_open`]) — so no mark is written and
//! nothing can drift. After a compaction removes those entries the file
//! is shown again, which is right: the model lost it.
//!
//! **The pinned head is untouched.** The append lands inside a
//! transcript entry, at the tail, which is the one cache-safe direction
//! (§5.5) — a context file never enters the frozen prefix.
//!
//! **Where it cannot reach.** The engine stats the agent's cwd. A
//! deployment that routes tools to a foot on another machine has its
//! cwd there, and the engine appends nothing; that foot may carry its
//! own discovery, which is not litany's to do.

use super::super::transcript;
use crate::config::Workflow;
use crate::prompt::Error;
use crate::prompt::tool::{context_file, frame_open};
use crate::template::GitRunner;
use crate::workspace;
use std::path::{Path, PathBuf};

/// Append every not-yet-shown context file to one tool result's
/// `content`, in path order — the repository's top level first, the
/// agent's own directory last, and within a directory the workflow's
/// declared name order.
///
/// A **settlement** never reaches here (`super::settle`): it renders no
/// envelope and states no exit code, because nothing ran and none is
/// invented, so it appends nothing either — the files ride the next
/// result, since nothing was marked shown.
pub(super) fn append(
    content: &mut Vec<u8>,
    conv_repo: &Path,
    worktree: &Path,
    conv_id: &str,
    workflow: &Workflow,
    git: &dyn GitRunner,
) -> Result<(), Error> {
    let found = discovered(conv_repo, conv_id, workflow, git);
    if found.is_empty() {
        return Ok(());
    }
    let entries = transcript::tool_entries(worktree)?;
    for (path, bytes) in found {
        if framed(&entries, &path) {
            continue;
        }
        // The frame is only meaningful on its own line — a result whose
        // last byte is not a newline gains a separator, the discipline
        // the stderr marker already follows.
        if !content.ends_with(b"\n") {
            content.push(b'\n');
        }
        content.extend_from_slice(&context_file(&path, &bytes, workflow.tool_output));
    }
    Ok(())
}

/// Every context file on the agent's cwd path, read. Presence and
/// content are one act: a name the workflow declares *is* a context
/// file exactly when this process can read it there, so a name that
/// does not exist, a directory wearing the name, and a file this
/// process may not open are one answer — absent.
fn discovered(
    conv_repo: &Path,
    conv_id: &str,
    workflow: &Workflow,
    git: &dyn GitRunner,
) -> Vec<(PathBuf, Vec<u8>)> {
    if workflow.context_files.is_empty() {
        return Vec::new();
    }
    let cwd = workspace::cwd::effective(conv_repo, conv_id, git);
    let cwd = std::fs::canonicalize(&cwd).unwrap_or(cwd);
    let mut found = Vec::new();
    for dir in path_set(&cwd, git) {
        for name in &workflow.context_files {
            let path = dir.join(name);
            if let Ok(bytes) = std::fs::read(&path) {
                found.push((path, bytes));
            }
        }
    }
    found
}

/// The directories a context file may sit in, top-level first: the
/// enclosing repository's top level down to `cwd` when `cwd` is inside
/// a repository, else `cwd` alone. A cwd outside any repository is the
/// general path with the enclosing tree absent — one directory, not a
/// special case.
fn path_set(cwd: &Path, git: &dyn GitRunner) -> Vec<PathBuf> {
    let Ok(top) = git.run_capture(cwd, &["rev-parse", "--show-toplevel"]) else {
        return vec![cwd.to_path_buf()];
    };
    let top = PathBuf::from(&top);
    let top = std::fs::canonicalize(&top).unwrap_or(top);
    // A toplevel that is not a prefix of the cwd cannot be walked down
    // to it; the cwd alone still answers, so no result is lost to a
    // path pair that disagrees.
    let Ok(rel) = cwd.strip_prefix(&top) else {
        return vec![cwd.to_path_buf()];
    };
    let mut walked = top;
    let mut dirs = vec![walked.clone()];
    for segment in rel.components() {
        walked = walked.join(segment);
        dirs.push(walked.clone());
    }
    dirs
}

/// Does any committed tool entry already frame `path`? The entries are
/// JSON, and the frame travels inside a string in them, so the needle
/// is the frame as JSON writes it — one derivation from
/// [`frame_open`], never a second spelling of the tag.
fn framed(entries: &[Vec<u8>], path: &Path) -> bool {
    let quoted = serde_json::to_string(&frame_open(path)).expect("a string serializes");
    let needle = quoted.trim_matches('"').as_bytes().to_vec();
    entries
        .iter()
        .any(|entry| entry.windows(needle.len()).any(|w| w == needle))
}
