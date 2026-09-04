//! Caller-supplied pinned documents (ARCH §2.5 "whatever documents the
//! dispatcher chose to pin", §2.3 step 2).
//!
//! `litany prompt --pin <dest>=<src>` and `litany dispatch --pin
//! <dest>=<src>` freeze exact caller-named bytes into the agent's
//! dispatch commit, beside `goal.md` and `soul.md` — standing context a
//! caller (a frontend, an operator) pins without rewriting the goal or
//! authoring a config commit. The mechanism is generic: litany owns
//! only the pinning — validation, snapshot, commit — and carries **no
//! filename policy**; which files count as project instructions,
//! their precedence and their size are the caller's concerns.
//!
//! **Validation is construction** ([`PinnedDoc::new`],
//! [`PinnedDocs::new`]): a destination is one worktree-relative path
//! that cannot traverse out (`..`, absolute, `.git`), cannot name a
//! harness-owned tree ([`reserved`]), and cannot collide with a sibling
//! pin — so the write path never re-checks, and a refusal happens in
//! the CLI layer before any branch, ref or inference exists. Provenance
//! needs no second copy: the pins are ordinary blobs on the dispatch
//! commit, inspectable with `git show`, and ordinary fork inheritance
//! carries them to descendants (§2.2). Whether a pinned document
//! composes into assembled context stays a §5.2 manifest question — the
//! destination the caller names is what the governing manifest's
//! `pinned:`/`order:` globs see.

use std::path::{Path, PathBuf};

/// Every way a pin can be refused. All fire in the CLI layer (or at
/// library construction), before any branch or ref exists.
#[derive(Debug, thiserror::Error)]
pub enum PinError {
    /// A `--pin` argument without the `<dest>=<source-path>` shape.
    #[error("pin {spec:?} must be <dest>=<source-path>")]
    Spec { spec: String },
    /// A destination that is not one safe worktree-relative path.
    #[error("pin destination {dest:?} {rule}")]
    Dest { dest: String, rule: String },
    /// Two pins naming one destination.
    #[error("pin destination {dest:?} is named twice")]
    Collision { dest: String },
    /// The source path's bytes could not be read.
    #[error("read pin source {path}: {source}")]
    Source {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
}

/// Worktree paths the harness itself writes, derives or reads
/// structurally, matched against a destination's first segment: the
/// system-slot files ([`crate::prompt::dispatch::step_commit::SYSTEM_SLOT_FILES`],
/// §2.3), the config
/// control files the dispatch commit removes
/// ([`crate::workspace::CONTROL_PATHS`], §2.2), the derived
/// `descriptions/**` (§3.3), the lineage's facts file
/// ([`crate::facts`], §5.5 — the dispatch commit cuts it out of the
/// governing config commit, so a pin there would be silently
/// overwritten by the very commit it rode in on), the transcript
/// (`messages/**`, §2.3) and the compaction chain (`summary/**`,
/// §2.7). A pin there would inject into a harness-owned tree, so it is
/// refused by name.
fn reserved(first_segment: &str) -> bool {
    let harness: &[&str] = &[
        crate::template::descriptions::DESCRIPTIONS_DIR,
        crate::prompt::dispatch::MESSAGES_DIR,
        crate::prompt::compactor::tools::SUMMARY_DIR,
        crate::facts::FILE,
    ];
    crate::workspace::CONTROL_PATHS.contains(&first_segment)
        || crate::prompt::dispatch::step_commit::SYSTEM_SLOT_FILES.contains(&first_segment)
        || harness.contains(&first_segment)
}

/// One caller-supplied pinned document: a validated worktree-relative
/// destination and the exact bytes to freeze there. Construction is the
/// only way in, so a held value is always writable as-is.
#[derive(Debug)]
pub struct PinnedDoc {
    dest: String,
    bytes: Vec<u8>,
}

impl PinnedDoc {
    /// Validate `dest` and take `bytes` verbatim. The destination must
    /// be a relative `/`-separated path with no empty, `.`, `..` or
    /// `.git` segment, whose first segment is no harness-owned name
    /// ([`reserved`]).
    pub fn new(dest: String, bytes: Vec<u8>) -> Result<Self, PinError> {
        let refuse = |rule: &str| PinError::Dest {
            dest: dest.clone(),
            rule: rule.to_owned(),
        };
        if dest.is_empty() {
            return Err(refuse("is empty"));
        }
        if dest.starts_with('/') {
            return Err(refuse("must be relative to the worktree root"));
        }
        for seg in dest.split('/') {
            if seg.is_empty() || seg == "." || seg == ".." {
                return Err(refuse("may not contain empty, '.' or '..' segments"));
            }
            if seg.eq_ignore_ascii_case(".git") {
                return Err(refuse("may not enter .git"));
            }
        }
        let first = dest.split('/').next().expect("split yields at least one");
        if reserved(first) {
            return Err(PinError::Dest {
                dest: dest.clone(),
                rule: format!("collides with the harness-owned path {first:?}"),
            });
        }
        Ok(Self { dest, bytes })
    }

    /// The validated worktree-relative destination.
    pub fn dest(&self) -> &str {
        &self.dest
    }
}

/// A collision-free set of pinned documents — the shape the verbs
/// thread through to the dispatch commit. [`PinnedDocs::none`] is the
/// empty default every pin-less start uses (the general path with empty
/// inputs, not a second shape).
#[derive(Debug)]
pub struct PinnedDocs(Vec<PinnedDoc>);

impl PinnedDocs {
    /// Wrap already-validated documents, refusing duplicate
    /// destinations — the one cross-document rule construction of a
    /// single [`PinnedDoc`] cannot see.
    pub fn new(docs: Vec<PinnedDoc>) -> Result<Self, PinError> {
        for (i, doc) in docs.iter().enumerate() {
            if docs[..i].iter().any(|d| d.dest == doc.dest) {
                return Err(PinError::Collision {
                    dest: doc.dest.clone(),
                });
            }
        }
        Ok(Self(docs))
    }

    /// The empty set — what a start with no `--pin` carries.
    pub fn none() -> &'static PinnedDocs {
        static NONE: PinnedDocs = PinnedDocs(Vec::new());
        &NONE
    }

    /// The documents, in caller order.
    pub fn iter(&self) -> impl Iterator<Item = &PinnedDoc> {
        self.0.iter()
    }

    /// Write every document under `worktree`, creating intermediate
    /// directories. Callers stage the same destinations into the
    /// dispatch commit's `git add`, so the snapshot is these bytes.
    ///
    /// A destination whose existing components — or whose final path —
    /// are symlinks is refused: the worktree already carries the forked
    /// tree, and a symlink there (agent- or template-authored, not the
    /// pinning caller's doing) would carry the write outside the
    /// worktree, voiding both the construction-time no-traversal rule
    /// and the byte-exact snapshot (the commit would hold the unchanged
    /// symlink, not the pinned bytes).
    pub(crate) fn write_into(&self, worktree: &Path) -> std::io::Result<()> {
        for doc in &self.0 {
            let mut path = worktree.to_path_buf();
            for seg in doc.dest.split('/') {
                path.push(seg);
                let is_link =
                    std::fs::symlink_metadata(&path).is_ok_and(|m| m.file_type().is_symlink());
                if is_link {
                    return Err(std::io::Error::other(format!(
                        "pin destination {:?} passes through a symlink at {:?}; \
                         refusing to write outside the worktree",
                        doc.dest, seg
                    )));
                }
            }
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::write(path, &doc.bytes)?;
        }
        Ok(())
    }
}

/// Parse and load `--pin <dest>=<source-path>` occurrences: split each
/// spec at its first `=` (a source path may contain `=`; a destination
/// may not), validate the destination, read the source's exact bytes,
/// and refuse collisions. Runs in the CLI layer, so every refusal
/// precedes the fork — no branch, ref or inference exists yet.
pub fn load(specs: &[String]) -> Result<PinnedDocs, PinError> {
    let mut docs = Vec::with_capacity(specs.len());
    for spec in specs {
        let (dest, src) = spec
            .split_once('=')
            .filter(|(d, s)| !d.is_empty() && !s.is_empty())
            .ok_or_else(|| PinError::Spec { spec: spec.clone() })?;
        let bytes = std::fs::read(src).map_err(|source| PinError::Source {
            path: PathBuf::from(src),
            source,
        })?;
        docs.push(PinnedDoc::new(dest.to_owned(), bytes)?);
    }
    PinnedDocs::new(docs)
}

#[cfg(test)]
mod tests;
