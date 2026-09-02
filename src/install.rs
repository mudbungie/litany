//! Installation-substrate seeding — the `litany prime` verb (ARCH §2.2).
//!
//! [`prime`] idempotently **founds the harness root**: it resolves the
//! config root and data root exactly as every other verb does (via
//! [`crate::harness_root`] — the XDG split, collapsed to one directory by
//! `LITANY_HOME`) and lays down what a ready installation carries — the
//! default `models.yaml` (ARCH §4.2), the `tools/` schema pool and the
//! `skills/` pool (ARCH §3.3), the `workflows/` template pool holding the
//! shipped default (`basic-agentic-loop.yaml`, ARCH §6) and the empty
//! `workspaces/` directory — **creating what is absent and never clobbering what
//! exists**. `models.yaml` is hand-edited by contract (§4.2), so it is
//! seeded only if absent; every pool entry is likewise seed-if-absent, so
//! a second run changes nothing and a hand-edited entry survives.
//!
//! The shipped assets are **embedded in the binary** at build time (the
//! same `include_dir!` discipline [`crate::template`] uses for the config
//! template), so the `litany` binary is self-contained: seeding a fresh
//! `LITANY_HOME` never reaches back to the source tree. `make install`
//! invokes `litany prime` rather than duplicating the seeding — the verb
//! is the single source of truth for what a ready installation looks like
//! (`docs/PRINCIPLES.md`, single source of truth; §3.4 front door).

use crate::harness_root::{Roots, models_path};
use crate::template::descriptions::{SKILLS_SUBDIR, TOOLS_SUBDIR};
use include_dir::{Dir, include_dir};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

/// Config-root subdir holding workflow templates (ARCH §2.2).
const WORKFLOWS_DIR: &str = "workflows";
/// The shipped default workflow's file name in that pool: the **basic
/// agentic loop** (ARCH §6, `docs/TAXONOMY.md` §1), under the name the
/// 2026-08-31 ruling gave it. The bytes are the config template's own
/// `workflow.yaml` — the declaration `litany new` freezes into every
/// `config/default` — so the pool's default entry and the freeze are one
/// asset read twice, never two declarations that can disagree.
const BASIC_AGENTIC_LOOP: &str = "basic-agentic-loop.yaml";
/// Data-root subdir holding the workspaces tree (ARCH §2.2).
const WORKSPACES_DIR: &str = "workspaces";

/// The default global `models.yaml` (ARCH §4.2), embedded verbatim.
const MODELS_YAML: &str = include_str!("../install/models.yaml");
/// The tool JSON-schema pool (ARCH §3.3 point 2), embedded as flat files.
static TOOLS: Dir<'_> = include_dir!("$CARGO_MANIFEST_DIR/schemas/tools");
/// The skill pool (ARCH §3.3), embedded as `<name>/SKILL.md` directories.
static SKILLS: Dir<'_> = include_dir!("$CARGO_MANIFEST_DIR/skills");

/// Why [`prime`] could not complete. The only failure is filesystem I/O;
/// the resolver's own failure is the caller's to surface (§3.4).
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("prime I/O at {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
}

fn io_err(path: &Path, source: io::Error) -> Error {
    Error::Io {
        path: path.to_path_buf(),
        source,
    }
}

/// What one [`prime`] run did to the harness root: the seed-if-absent
/// split, counted. It carries no paths — the roots are the caller's own
/// [`Roots`], and re-stating them here would be a second copy of one fact
/// (`docs/PRINCIPLES.md`, single source of truth).
#[derive(Debug, Default, Clone, Copy)]
pub struct Founding {
    /// Files this run wrote, because they were absent.
    pub seeded: usize,
    /// Files already on disk, left byte-for-byte alone.
    pub kept: usize,
}

impl Founding {
    /// Record one seed decision: `wrote` is [`seed_file`]'s answer.
    fn count(&mut self, wrote: bool) {
        if wrote {
            self.seeded += 1;
        } else {
            self.kept += 1;
        }
    }
}

/// Found the harness root (ARCH §2.2), seed-if-absent throughout, and
/// report the split (see [`Founding`]) so the caller can say what it did.
///
/// Idempotent by construction: directories are `create_dir_all` (a no-op
/// when present) and every file is written only when absent, so a second
/// run changes nothing and a hand-edited entry (a curated `models.yaml`,
/// an edited `SKILL.md`) is never clobbered. Under `LITANY_HOME` both
/// roots collapse to one directory and every path below lands there.
pub fn prime(roots: &Roots) -> Result<Founding, Error> {
    let mut founding = Founding::default();
    // Config lifetime (§2.2): the hand-edited declarations.
    let workflows = roots.config.join(WORKFLOWS_DIR);
    ensure_dir(&workflows)?;
    founding.count(seed_file(
        &workflows.join(BASIC_AGENTIC_LOOP),
        basic_agentic_loop(),
    )?);
    founding.count(seed_file(
        &models_path(&roots.config),
        MODELS_YAML.as_bytes(),
    )?);

    // Data lifetime (§2.2): the machine-populated pools and trees.
    ensure_dir(&roots.data.join(WORKSPACES_DIR))?;
    extract_dir(&TOOLS, &roots.data.join(TOOLS_SUBDIR), &mut founding)?;
    extract_dir(&SKILLS, &roots.data.join(SKILLS_SUBDIR), &mut founding)?;
    Ok(founding)
}

/// The basic agentic loop's bytes, read out of the embedded config
/// template rather than embedded a second time — one asset, two seeding
/// paths (the freeze at `litany new`, this pool entry). The template
/// always ships it (`crate::template::TEMPLATE`, pinned by
/// `tests::shipped_template`), so the `None` arm is a programmer error.
fn basic_agentic_loop() -> &'static [u8] {
    crate::template::TEMPLATE
        .get_file(crate::prompt::WORKFLOW_FILE)
        .expect("the config template ships a workflow.yaml")
        .contents()
}

/// Recursively seed an embedded directory into `target`, seed-if-absent
/// per leaf file. `target` is the on-disk directory that mirrors `dir`;
/// each embedded file lands at `target/<file-name>`, each embedded subdir
/// recurses into `target/<subdir-name>`. Only files are seed-guarded —
/// directories are `create_dir_all`, so an existing pool with extra
/// entries keeps them and gains only what the binary ships and disk lacks.
fn extract_dir(dir: &Dir, target: &Path, founding: &mut Founding) -> Result<(), Error> {
    ensure_dir(target)?;
    for file in dir.files() {
        let name = leaf_name(file.path());
        founding.count(seed_file(&target.join(name), file.contents())?);
    }
    for sub in dir.dirs() {
        let name = leaf_name(sub.path());
        extract_dir(sub, &target.join(name), founding)?;
    }
    Ok(())
}

/// The final path component of an embedded entry. Embedded paths always
/// have a name (the macro never yields a rootless entry), so the `None`
/// arm is a programmer-error panic, excluded from coverage.
fn leaf_name(path: &Path) -> &std::ffi::OsStr {
    path.file_name().expect("embedded entry has a name")
}

/// Write `bytes` to `path` iff `path` is absent; a present path is left
/// untouched (seed-if-absent — the non-clobber contract, §2.2). Parity
/// with the Makefile's retired `test -e` guard on `models.yaml`. Answers
/// whether it wrote, which is the whole of [`Founding`].
fn seed_file(path: &Path, bytes: &[u8]) -> Result<bool, Error> {
    if path.exists() {
        return Ok(false);
    }
    fs::write(path, bytes).map_err(|e| io_err(path, e))?;
    Ok(true)
}

/// `create_dir_all`, mapping the error to [`Error::Io`] with the path.
fn ensure_dir(dir: &Path) -> Result<(), Error> {
    fs::create_dir_all(dir).map_err(|e| io_err(dir, e))
}

#[cfg(test)]
mod tests;
