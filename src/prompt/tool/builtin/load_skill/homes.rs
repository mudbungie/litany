//! Where a skill body comes from (ARCH §3.3,
//! `docs/DESIGN_LEARNING_LOOP.md` §3): the two homes an election
//! resolves over, and the plumbing that reads each.
//!
//! The **followed config commit** is the workspace's home — a body
//! committed in the lineage at `skills/<name>/`, versioned and forkable
//! with it. The **install pool** is `<data-root>/skills/<name>/`,
//! shared by every workspace on the box. Ownership is the path; the
//! order is the config commit first, the pool second, and the
//! config-authoring pass is what makes that order total by refusing a
//! name both homes hold ([`crate::template::descriptions`]).
//!
//! A decline names *both* sets, because a reader who mistyped needs to
//! know which two were searched.

use super::super::dispatch::EnvLookup;
use super::{ENV_HOME, ENV_LITANY_HOME, ENV_XDG_DATA, Error, SKILLS_DIR};
use crate::harness_root;
use crate::template::{GitRunner, descriptions};
use crate::workspace;
use std::io;
use std::path::{Path, PathBuf};

/// The **followed config commit** of the calling agent's branch (ARCH
/// §2.2, `crate::workspace::current_config`) — the same tip control
/// resolves from at every step boundary, so an accepted config edit
/// reaches an election with no act per agent. The held arm (diverged
/// lineages) answers the fork commit, exactly as resolution does.
pub(super) fn followed_commit(
    workspace: &Path,
    branch: &str,
    git: &dyn GitRunner,
) -> Result<String, Error> {
    let rev = workspace::agent_ref(branch);
    let resolution =
        workspace::current_config::current_config(workspace, &rev, git).map_err(Error::Lineage)?;
    Ok(resolution.commit().to_owned())
}

/// The decline for a name neither home holds, naming **both** pools —
/// the "name the pool" idiom ([`crate::name::pool`]) over two homes,
/// because a reader who mistyped needs to know which sets were searched.
pub(super) fn unknown(
    name: String,
    worktree: &Path,
    commit: &str,
    pool: &Path,
    git: &dyn GitRunner,
) -> Error {
    Error::Unknown {
        name,
        workspace: committed_skills(worktree, commit, git),
        pool: available(pool),
    }
}

/// Comma-joined names of the followed config commit's workspace skills,
/// read from the commit's tree. The archive container is not a skill and
/// is not listed. An unreadable tree renders as the empty pool.
fn committed_skills(worktree: &Path, commit: &str, git: &dyn GitRunner) -> String {
    let spec = format!("{commit}:{SKILLS_DIR}");
    let listing = git
        .run_capture(worktree, &["ls-tree", "--name-only", &spec])
        .unwrap_or_default();
    let names: Vec<&str> = listing
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && *l != descriptions::ARCHIVED_SUBDIR)
        .collect();
    crate::name::pool(&names)
}

/// The data-root skills pool `<data-root>/skills/` (§3.3). Resolved from
/// the same env [`harness_root`] reads, injected via [`EnvLookup`] so the
/// tool stays pure over its environment for tests.
pub(super) fn skills_pool(env: &dyn EnvLookup) -> Result<PathBuf, Error> {
    let override_v = env.get(ENV_LITANY_HOME);
    let xdg_data = env.get(ENV_XDG_DATA);
    let home = env.get(ENV_HOME);
    let roots = harness_root::resolve_from(
        override_v.as_deref(),
        None,
        xdg_data.as_deref(),
        home.as_deref().map(Path::new),
    )
    .map_err(Error::Root)?;
    Ok(roots.data.join(SKILLS_DIR))
}

/// Comma-joined, sorted list of the install pool's skill directory names
/// for the decline message, rendered by the shared [`crate::name::pool`]
/// idiom. A missing or unreadable pool reads as `(none)` — the decline
/// still names *that* there is nothing to load.
fn available(pool: &Path) -> String {
    let mut names: Vec<String> = match std::fs::read_dir(pool) {
        Ok(rd) => rd
            .flatten()
            .filter(|e| e.path().is_dir())
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .collect(),
        Err(_) => Vec::new(),
    };
    names.sort();
    crate::name::pool(&names)
}

/// Recursively copy `src` into `dest`, creating `dest` and any parents.
/// Plain byte copies of files under a mirrored directory tree — the same
/// portability discipline `make install` uses for the pool (§3.3).
pub(super) fn copy_dir(src: &Path, dest: &Path) -> io::Result<()> {
    std::fs::create_dir_all(dest)?;
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let from = entry.path();
        let to = dest.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            copy_dir(&from, &to)?;
        } else {
            std::fs::copy(&from, &to)?;
        }
    }
    Ok(())
}
