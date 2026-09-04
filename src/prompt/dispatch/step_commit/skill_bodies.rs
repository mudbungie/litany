//! Drop the **config lineage's** skill bodies from a forked tree (ARCH
//! §2.2, §3.3; `docs/DESIGN_LEARNING_LOOP.md` §3).
//!
//! A config commit may carry **workspace skills** at `skills/<name>/` —
//! bodies the workspace owns, versioned in the lineage and forkable with
//! it. A fork off that commit inherits them like any other tree content,
//! and must not: **a body is not context until it is elected.** What a
//! fork inherits is the *description* (`descriptions/skills/<name>.md`,
//! the descriptor cut beside this one); `load_skill` is what turns a
//! description into a body, and it checks the body out of this same
//! commit when the agent asks for it (§3.3 *Body-on-demand*).
//!
//! **Exactly the names the commit shares with the tree, never `skills/`
//! whole.** An agent's `skills/` is also where its own elected bodies
//! live, and §2.7 makes those the compactor's input — a fork that
//! removed the directory outright would take a parent's spent skills
//! away from the child forked to compact them. So the removal is the
//! intersection: what the lineage contributed leaves, what an agent
//! elected stays.
//!
//! Read from the **tree** and tested against the commit, the shape
//! [`super::descriptors`]'s `ungranted` uses and for the same reason: a
//! tree with no `skills/` is the ordinary case (every fork off a lineage
//! that has authored no workspace skill, and every stub-git fixture), and
//! it must cost no git command at all.

use crate::prompt::Error;
use crate::template::GitRunner;
use crate::workspace::SKILLS_DIR;
use std::path::Path;

/// Stage the removal of every `skills/<name>` the forked tree carries
/// that the governing config commit also carries.
pub(super) fn drop_config_bodies(
    worktree: &Path,
    config_commit: &str,
    git: &dyn GitRunner,
) -> Result<(), Error> {
    let paths: Vec<String> = inherited(worktree)?
        .into_iter()
        .map(|name| format!("{SKILLS_DIR}/{name}"))
        .filter(|path| {
            let spec = format!("{config_commit}:{path}");
            git.run(worktree, &["cat-file", "-e", &spec]).is_ok()
        })
        .collect();
    if paths.is_empty() {
        return Ok(());
    }
    let mut args: Vec<&str> = vec!["rm", "-r", "-q", "--ignore-unmatch", "--"];
    args.extend(paths.iter().map(String::as_str));
    git.run(worktree, &args).map_err(|source| Error::Git {
        op: "rm the config's skill bodies",
        source,
    })
}

/// The names directly under the forked tree's `skills/`, sorted so the
/// staged removal is deterministic. No `skills/` at all yields none.
fn inherited(worktree: &Path) -> Result<Vec<String>, Error> {
    let entries = match std::fs::read_dir(worktree.join(SKILLS_DIR)) {
        Ok(iter) => iter,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(e.into()),
    };
    let mut names: Vec<String> = entries
        .flatten()
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    Ok(names)
}
