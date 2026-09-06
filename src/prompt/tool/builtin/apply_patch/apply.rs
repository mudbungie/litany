//! Application of a parsed patch (ARCH §3.3 *The patch tool*).
//!
//! Two phases, which is where atomicity lives: **stage** reads every
//! target and computes every post-state in memory — every decline
//! (missing file, existing add target, context not found or ambiguous)
//! fires here, before any byte on disk has changed — then **write**
//! lands the staged states. A patch that cannot apply in full applies
//! not at all. (A write-phase I/O fault can still stop midway; it is
//! surfaced verbatim, and the per-invocation tool commit records the
//! exact resulting tree either way.)
//!
//! The stale-state guard (bl-e249) is structural, not a version number:
//! an add declines when the path already exists, an update or delete
//! declines when it does not, and an update's hunks must locate their
//! authored context through the [`super::seek`] matching ladder —
//! uniquely. Content that drifted since the model last read it stops
//! matching and the patch is refused with the exact reason, never
//! applied over unseen changes.
//!
//! Written destinations also honor the write-time symlink discipline
//! (bl-91f8, bl-2502): a destination that is itself a symlink —
//! dangling or not — is declined for add, update, and rename targets,
//! because `fs::write` follows the link and would land the bytes
//! outside the authored path (a dangling link even reads as vacant to
//! `exists()`, silently bypassing the add guard). Delete is exempt:
//! `fs::remove_file` removes the link itself, never its target.

use super::parse::{FileOp, Patch};
use super::report::{FileReport, Report, entry};
use super::seek;
use super::splice::run_hunks;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use thiserror::Error;

/// Why the patch was refused. Every variant names the file, the hunk
/// where applicable, and the exact reason — the model repairs from this
/// text alone.
#[derive(Debug, Error)]
pub enum Error {
    /// The add target already exists: the patch was authored against a
    /// tree that did not have it, so applying would overwrite unseen
    /// content. Update it or delete it first.
    #[error("add {path}: file already exists; update it or delete it first")]
    AddExists { path: String },
    /// Reading a target failed — missing file (the stale-state case for
    /// update/delete), permission, or non-UTF-8 content (the tool edits
    /// text; binary files stay `bash`'s business).
    #[error("{action} {path}: {source}")]
    Io {
        action: &'static str,
        path: String,
        #[source]
        source: io::Error,
    },
    /// The rename target of a `*** Move to:` already exists.
    #[error("move {path} to {to}: destination already exists")]
    MoveDestExists { path: String, to: String },
    /// The destination path itself is a symlink — dangling or not. A
    /// plain `exists()` follows links, so a dangling link reads as
    /// vacant while the write would land through it, outside the
    /// authored path: the write-time symlink discipline (bl-91f8)
    /// declines instead (bl-2502).
    #[error(
        "{action} {path}: destination is a symlink; refusing to write \
         through it — name the link's target directly, or delete the \
         link first"
    )]
    SymlinkDest { action: &'static str, path: String },
    /// A hunk's anchor or context block was not found from the cursor
    /// on, at any rung of the matching ladder.
    #[error("update {path}, hunk {hunk}: {what} {source}")]
    NotFound {
        path: String,
        hunk: usize,
        what: String,
        source: seek::Error,
    },
    /// The first rung to match at all matched more than once: a loud
    /// decline, never a guessed edit. `@@ <enclosing symbol>` anchor
    /// lines or more context make the target unique.
    #[error(
        "update {path}, hunk {hunk}: {what} is ambiguous — {source}; \
         add an `@@ <enclosing symbol>` anchor line or more context"
    )]
    Ambiguous {
        path: String,
        hunk: usize,
        what: String,
        source: seek::Error,
    },
    /// A pure-insertion hunk (no context, no removals) with neither an
    /// `@@` anchor nor `*** End of File` has no defined landing point.
    #[error(
        "update {path}, hunk {hunk}: insertion has no location; give it \
         context lines, an `@@` anchor, or `*** End of File`"
    )]
    UnanchoredInsertion { path: String, hunk: usize },
}

/// A staged operation: the fully computed post-state, ready to write.
enum Staged {
    Add {
        abs: PathBuf,
        report: FileReport,
        content: String,
    },
    Delete {
        abs: PathBuf,
        report: FileReport,
    },
    Update {
        abs: PathBuf,
        move_abs: Option<PathBuf>,
        report: FileReport,
        content: String,
    },
}

/// Apply `patch` with paths resolved against `root` (the calling
/// agent's current working directory; an absolute patch path stands as
/// itself, `Path::join` semantics). All-or-nothing per the module doc.
pub fn apply(patch: &Patch, root: &Path) -> Result<Report, Error> {
    let staged: Vec<Staged> = patch
        .ops
        .iter()
        .map(|op| stage(op, root))
        .collect::<Result<_, _>>()?;
    let files = staged.into_iter().map(write).collect::<Result<_, _>>()?;
    Ok(Report {
        status: "applied",
        root: root.display().to_string(),
        files,
    })
}

/// Phase one: validate one operation and compute its post-state.
fn stage(op: &FileOp, root: &Path) -> Result<Staged, Error> {
    match op {
        FileOp::Add { path, lines } => {
            let abs = root.join(path);
            refuse_symlink("add", &abs, path)?;
            if abs.exists() {
                return Err(Error::AddExists { path: path.clone() });
            }
            // The parser guarantees at least one content line
            // ([`super::parse::Error::EmptyAdd`], bl-c4a2), so the added
            // file always ends in a newline and the empty-content arm
            // that used to sit here is gone with the state it served.
            Ok(Staged::Add {
                abs,
                report: entry(path, "add", None, Vec::new()),
                content: lines.join("\n") + "\n",
            })
        }
        FileOp::Delete { path } => {
            let abs = root.join(path);
            // The read is the existence guard — and preserves the
            // pre-state in the staged view before removal.
            read(&abs, path)?;
            Ok(Staged::Delete {
                abs,
                report: entry(path, "delete", None, Vec::new()),
            })
        }
        FileOp::Update {
            path,
            move_to,
            hunks,
        } => {
            let abs = root.join(path);
            refuse_symlink("update", &abs, path)?;
            let text = read(&abs, path)?;
            let move_abs = match move_to {
                Some(to) => {
                    let dest = root.join(to);
                    refuse_symlink("move to", &dest, to)?;
                    if dest.exists() {
                        return Err(Error::MoveDestExists {
                            path: path.clone(),
                            to: to.clone(),
                        });
                    }
                    Some(dest)
                }
                None => None,
            };
            let (content, applied) = run_hunks(&text, hunks, path)?;
            Ok(Staged::Update {
                abs,
                move_abs,
                report: entry(path, "update", move_to.clone(), applied),
                content,
            })
        }
    }
}

/// Decline when the final path is itself a symlink — dangling or not —
/// per the write-time symlink discipline (bl-91f8): `fs::write` follows
/// the link, so the bytes would land outside the authored path while
/// the guards (`exists()`, the staged read) reported on the target.
/// Delete needs no guard: `fs::remove_file` removes the link itself,
/// never its target.
fn refuse_symlink(action: &'static str, abs: &Path, path: &str) -> Result<(), Error> {
    let is_link = fs::symlink_metadata(abs).is_ok_and(|m| m.file_type().is_symlink());
    if is_link {
        return Err(Error::SymlinkDest {
            action,
            path: path.to_string(),
        });
    }
    Ok(())
}

fn read(abs: &Path, path: &str) -> Result<String, Error> {
    fs::read_to_string(abs).map_err(|source| Error::Io {
        action: "read",
        path: path.to_string(),
        source,
    })
}

fn io_err(action: &'static str, abs: &Path) -> impl FnOnce(io::Error) -> Error {
    let path = abs.display().to_string();
    move |source| Error::Io {
        action,
        path,
        source,
    }
}

/// Phase two: land one staged operation on disk.
fn write(staged: Staged) -> Result<FileReport, Error> {
    match staged {
        Staged::Add {
            abs,
            report,
            content,
        } => {
            if let Some(parent) = abs.parent().filter(|p| !p.as_os_str().is_empty()) {
                fs::create_dir_all(parent).map_err(io_err("create directory for", &abs))?;
            }
            fs::write(&abs, content).map_err(io_err("write", &abs))?;
            Ok(report)
        }
        Staged::Delete { abs, report } => {
            fs::remove_file(&abs).map_err(io_err("delete", &abs))?;
            Ok(report)
        }
        Staged::Update {
            abs,
            move_abs,
            report,
            content,
        } => {
            fs::write(&abs, content).map_err(io_err("write", &abs))?;
            if let Some(dest) = move_abs {
                if let Some(parent) = dest.parent().filter(|p| !p.as_os_str().is_empty()) {
                    fs::create_dir_all(parent).map_err(io_err("create directory for", &dest))?;
                }
                fs::rename(&abs, &dest).map_err(io_err("rename", &abs))?;
            }
            Ok(report)
        }
    }
}
