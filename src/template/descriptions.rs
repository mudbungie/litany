//! Descriptions-always producer (ARCH §3.3 *Descriptions-always
//! population*). A single step of the creation routine
//! ([`super::scaffold`]) snapshots the data-root pools into the
//! worktree's `descriptions/**` so every agent branch inherits them via
//! git (§2.2, §2.3) and context assembly intersects a role's declared
//! tools against a committed, immutable schema set rather than re-reading
//! mutable data-root state (§2.10, §5.1) — the committed form of the
//! §2.2 control discipline (control lives in config commits; the
//! commit read is the lineage's followed tip since bl-403b).
//!
//! **One mechanism over two artifact kinds, not two producers:** the
//! same pass copies every available tool's JSON schema
//! (`<data-root>/tools/<name>.json` → `descriptions/tools/<name>.json`,
//! verbatim) and every available skill's `SKILL.md` frontmatter
//! (`<data-root>/skills/<name>/SKILL.md` → `descriptions/skills/<name>.md`).
//!
//! The data-root pools are the single source of truth for *what this
//! install provides*; the committed `descriptions/**` snapshot is the
//! single source of truth for *what agents forked from this config are
//! pinned to see* — distinct facts, so the copy is a snapshot, not a
//! mirror (`docs/PRINCIPLES.md`, single source of truth). An empty (or
//! absent) pool yields an empty descriptions tree, which the composer
//! (`crate::prompt::dispatch::tools`) reads as an empty toolset.
//!
//! **Validated at snapshot time, with the composer's own parsers
//! (bl-e3f5).** A malformed pooled artifact — a `SKILL.md` frontmatter
//! block whose YAML does not parse (the `description: foo: bar`
//! plain-scalar trap), or a tool schema that is not valid JSON — used to
//! pass this snapshot unparsed and surface only at the first prompt step,
//! deep inside `crate::prompt::dispatch::tools::compose` (ARCH §3.3
//! *Tools-list assembly*), after `litany new` or `litany config` had
//! already authored the commit (and, for `new`, created the workspace).
//! This pass now runs the frontmatter YAML through the same
//! [`skill::parse`] the composer's `read_description` calls, and the
//! schema JSON through the same `serde_json::from_slice` its
//! `read_schema` calls — one parser per artifact kind, shared by producer
//! and consumer, so a malformed pool file is refused here, before any
//! commit lands, naming the offending pool file rather than silently
//! shipping bytes prompt-time will later reject (single source of truth:
//! `docs/PRINCIPLES.md`).

use crate::skill;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

/// Worktree-relative root under which the snapshot lands. Kept in step
/// with the composer's `descriptions/tools` and with ARCH §2.2's layout.
pub const DESCRIPTIONS_DIR: &str = "descriptions";
/// Pool + descriptions subdir holding tool JSON schemas.
pub const TOOLS_SUBDIR: &str = "tools";
/// Pool + descriptions subdir holding skill frontmatter.
pub const SKILLS_SUBDIR: &str = "skills";
/// The frontmatter-bearing file inside each skill directory.
pub const SKILL_MANIFEST: &str = "SKILL.md";
/// Extension of a tool schema in the pool (copied verbatim).
const JSON_EXT: &str = "json";

/// Why [`snapshot`] could not complete.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("descriptions I/O at {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("skill {name}: {} has no YAML frontmatter block", SKILL_MANIFEST)]
    NoFrontmatter { name: String },
    #[error("skill {name}: {path} frontmatter is malformed: {source}")]
    SkillFrontmatter {
        name: String,
        path: PathBuf,
        #[source]
        source: serde_yaml_ng::Error,
    },
    #[error("tool {name}: {path} is not valid JSON: {source}")]
    ToolSchema {
        name: String,
        path: PathBuf,
        #[source]
        source: serde_json::Error,
    },
}

fn io_err(path: &Path, source: io::Error) -> Error {
    Error::Io {
        path: path.to_path_buf(),
        source,
    }
}

/// Snapshot the data-root tool schemas and skill frontmatter into
/// `<worktree>/descriptions/{tools,skills}/`. Idempotent overwrite; a
/// missing pool directory is not an error (empty pool → empty
/// descriptions tree, §3.3).
pub fn snapshot(data_root: &Path, worktree: &Path) -> Result<(), Error> {
    copy_tool_schemas(&data_root.join(TOOLS_SUBDIR), worktree)?;
    copy_skill_frontmatter(&data_root.join(SKILLS_SUBDIR), worktree)?;
    Ok(())
}

/// Copy every `<pool>/<name>.json` to `<worktree>/descriptions/tools/<name>.json`
/// (§3.3 point 2), verbatim once validated: parsed with the same
/// `serde_json::from_slice` the composer's `read_schema` runs at prompt
/// time (bl-e3f5), so a malformed schema is declined here rather than
/// snapshotted and rejected three steps later.
fn copy_tool_schemas(pool: &Path, worktree: &Path) -> Result<(), Error> {
    let dest = worktree.join(DESCRIPTIONS_DIR).join(TOOLS_SUBDIR);
    for entry in read_pool(pool)? {
        let path = entry.path();
        if !path.is_file() || path.extension().and_then(|e| e.to_str()) != Some(JSON_EXT) {
            continue;
        }
        let raw = fs::read(&path).map_err(|e| io_err(&path, e))?;
        let name = path
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_default();
        serde_json::from_slice::<serde_json::Value>(&raw).map_err(|source| Error::ToolSchema {
            name,
            path: path.clone(),
            source,
        })?;
        ensure_dir(&dest)?;
        let target = dest.join(entry.file_name());
        fs::write(&target, &raw).map_err(|e| io_err(&target, e))?;
    }
    Ok(())
}

/// Extract each `<pool>/<name>/SKILL.md`'s frontmatter and write it to
/// `<worktree>/descriptions/skills/<name>.md` (§3.3 *Description-always*).
/// The extracted body is parsed with the same [`skill::parse`] the
/// composer's `read_description` runs at prompt time (bl-e3f5) — the
/// fence-detection [`skill::frontmatter_yaml`] alone does not catch a
/// malformed YAML body (e.g. an unquoted `description: foo: bar`, the
/// plain-scalar trap), only a missing or unclosed fence.
fn copy_skill_frontmatter(pool: &Path, worktree: &Path) -> Result<(), Error> {
    let dest = worktree.join(DESCRIPTIONS_DIR).join(SKILLS_SUBDIR);
    for entry in read_pool(pool)? {
        let dir = entry.path();
        if !dir.is_dir() {
            continue;
        }
        let manifest = dir.join(SKILL_MANIFEST);
        let raw = match fs::read_to_string(&manifest) {
            Ok(s) => s,
            // A directory with no SKILL.md is not an available skill.
            Err(e) if e.kind() == io::ErrorKind::NotFound => continue,
            Err(e) => return Err(io_err(&manifest, e)),
        };
        let name = entry.file_name().to_string_lossy().into_owned();
        let body =
            skill::frontmatter_yaml(&raw).ok_or(Error::NoFrontmatter { name: name.clone() })?;
        skill::parse(body).map_err(|source| Error::SkillFrontmatter {
            name: name.clone(),
            path: manifest.clone(),
            source,
        })?;
        ensure_dir(&dest)?;
        let target = dest.join(format!("{name}.md"));
        fs::write(&target, body).map_err(|e| io_err(&target, e))?;
    }
    Ok(())
}

/// Read a pool directory into a name-sorted vec of entries; a missing
/// pool is an empty pool (§3.3), never an error. Sorting makes the
/// snapshot order deterministic. Individual entries that fail to
/// enumerate (a transient per-entry `read_dir` error) are skipped via
/// `flatten` — the snapshot is a set of independent files, so a dropped
/// entry degrades to compose dropping that tool, never a corrupt tree.
fn read_pool(pool: &Path) -> Result<Vec<fs::DirEntry>, Error> {
    let iter = match fs::read_dir(pool) {
        Ok(it) => it,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(io_err(pool, e)),
    };
    let mut entries: Vec<fs::DirEntry> = iter.flatten().collect();
    entries.sort_by_key(|e| e.file_name());
    Ok(entries)
}

fn ensure_dir(dir: &Path) -> Result<(), Error> {
    fs::create_dir_all(dir).map_err(|e| io_err(dir, e))
}

#[cfg(test)]
mod tests;
