//! Derive the dispatched agent's descriptor tree from its **governing
//! config commit**, filtered to its role's grant (ARCH §3.3, §5.1, §2.3
//! step 2).
//!
//! `descriptions/**` is snapshotted **whole** into the config commit —
//! every tool's schema and every skill's frontmatter the install
//! provides (§3.3 *Descriptions-always population*) — because one config
//! commit serves every role. That commit is the authoritative descriptor
//! set; an agent's own tree is a *derived* view of it, cut to the role's
//! `tools:` grant (§4.3), and the dispatch commit is where the cut is
//! made.
//!
//! **Derived from the config commit, never from the parent's tree.**
//! Pruning the forked-in tree instead capped every child's descriptors at
//! its dispatcher's: the tree a child forks off was already cut to the
//! *parent's* grant, so a chain of dispatches intersected grant after
//! grant and a child's own `tools:` could never widen one. Reproduced
//! (bl-a900): a sensor role granted `[slack_read, message]`, dispatched
//! by a worker whose grant lacked `slack_read`, composed a request naming
//! neither — the role's one instrument gone with no diagnostic anywhere,
//! because tools-list assembly (§3.3) intersects declaration against the
//! tree and drops what the tree does not carry. Deriving from the
//! governing config commit makes an agent's tree a function of *its own*
//! grant alone, so dispatch order cannot narrow it.
//!
//! **A grant with no descriptor is declined, not composed smaller.** A
//! role granting a tool the governing config commit does not describe is
//! a config fault — `providers.yaml` and `descriptions/**` disagree, and
//! both live in that one commit. It is refused at the fork, naming the
//! tool and the described pool, before a branch, worktree or inbox exists
//! — the validity-before-fork discipline of role validation (§4.3) and
//! the §6 budget gate.
//!
//! **What the cut leaves is exact.** A non-granted tool's descriptors
//! compose **nowhere**: the body walk skips `descriptions/tools/**` and
//! every tool-claimed skill description (§3.3 *two wire homes*), and the
//! tools array carries only what the role declared. Uncomposed worktree
//! bytes are ordinary — the manifest is the inclusion list (§5.1) — but
//! these ones purported to describe the callable set, and they are
//! reachable by `bash`, which is how the failure was found (yog
//! bl-55b1): an agent read `descriptions/tools/message.json`, concluded
//! the environment supported messaging, and spent many steps discovering
//! that its wire array said otherwise. After the cut,
//! `descriptions/tools/` **is** the callable set, answered from the
//! agent's own branch in one listing.
//!
//! **Standalone skills stay.** Only a skill some tool claims — one with a
//! `descriptions/tools/<name>.json` beside it — leaves with its tool. A
//! skill no tool claims composes as a path-framed head text block (§3.3,
//! §5.2) and is `load_skill`-able; it is granted by being present.

use crate::prompt::Error;
use crate::template::GitRunner;
use std::path::Path;

/// Worktree-relative home of the committed tool schemas (§3.3).
const TOOLS_DIR: &str = "descriptions/tools";
/// Worktree-relative home of the committed skill frontmatter (§3.3).
const SKILLS_DIR: &str = "descriptions/skills";

/// A role grants a tool the governing config commit does not describe
/// (§3.3) — `providers.yaml` and `descriptions/**` disagree inside one
/// commit. Its own type, like role validity's
/// ([`crate::prompt::role::validate::Invalid`]): the decline's vocabulary
/// belongs with the check that raises it, and the pool it names is read
/// only here.
#[derive(Debug, thiserror::Error)]
#[error(
    "role {role:?} grants tool {tool:?}, which its governing config commit does not \
     describe — no descriptions/tools/{tool}.json (ARCH §3.3 descriptions-always); \
     described tools: {described}"
)]
pub(crate) struct Undescribed {
    pub(crate) role: String,
    pub(crate) tool: String,
    pub(crate) described: String,
}

/// What one dispatch commit cuts an agent's descriptor tree to (§2.3
/// step 2): a role, its `tools:` grant, and the config commit both were
/// read from. The three travel together because the cut means nothing
/// without all of them — the grant selects, the commit supplies, and the
/// role is what a refusal names.
pub(crate) struct Grant<'a> {
    /// The dispatched agent's role (§4.3).
    pub(crate) role: &'a str,
    /// The role's `tools:` grant, read from `config_commit`'s
    /// `providers.yaml`. Empty is the compactor's ordinary shape (§2.7),
    /// not a missing value.
    pub(crate) tools: &'a [String],
    /// The **governing config commit** (§2.2) — the authoritative
    /// `descriptions/**` snapshot the agent's tree derives from.
    pub(crate) config_commit: &'a str,
}

/// Cut the forked tree's descriptors to `grant`, derived from the
/// governing config commit: decline an undescribed grant, drop what the
/// grant does not cover, check out what it does.
///
/// Order is load-bearing only at the head — a fork that cannot be
/// described correctly changes nothing. Drop and check-out name disjoint
/// tools by construction.
pub(crate) fn derive(worktree: &Path, grant: &Grant<'_>, git: &dyn GitRunner) -> Result<(), Error> {
    require_described(worktree, grant, git)?;
    drop_ungranted(worktree, grant.tools, git)?;
    checkout_granted(worktree, grant, git)
}

/// Decline the dispatch when the governing config commit carries no
/// schema for some granted tool (§3.3). Runs in any checkout onto the
/// workspace's object store, so a caller pre-flights it *before* the
/// fork and a refusal leaves no branch debris.
pub(crate) fn require_described(
    dir: &Path,
    grant: &Grant<'_>,
    git: &dyn GitRunner,
) -> Result<(), Undescribed> {
    for tool in grant.tools {
        if !committed(dir, grant.config_commit, &schema_path(tool), git) {
            return Err(Undescribed {
                role: grant.role.to_owned(),
                tool: tool.clone(),
                described: described_pool(dir, grant.config_commit, git),
            });
        }
    }
    Ok(())
}

/// Check out every granted tool's schema — and its claimed skill
/// frontmatter, where the commit carries one — from the config commit.
/// `git checkout <commit> -- <paths>` writes *and* stages, so the
/// dispatch commit carries them with no second `add`.
///
/// Unconditional over the whole grant rather than only over what the
/// forked-in tree lacks: the config commit is the descriptor set's one
/// home (`docs/PRINCIPLES.md` Single source of truth), so what the fork
/// point happened to carry is never consulted. An empty grant checks out
/// nothing — the compactor's shape.
fn checkout_granted(worktree: &Path, grant: &Grant<'_>, git: &dyn GitRunner) -> Result<(), Error> {
    let mut paths: Vec<String> = Vec::new();
    for tool in grant.tools {
        paths.push(schema_path(tool));
        let skill = skill_path(tool);
        if committed(worktree, grant.config_commit, &skill, git) {
            paths.push(skill);
        }
    }
    if paths.is_empty() {
        return Ok(());
    }
    let mut args: Vec<&str> = vec!["checkout", grant.config_commit, "--"];
    args.extend(paths.iter().map(String::as_str));
    git.run(worktree, &args).map_err(|source| Error::Git {
        op: "checkout granted descriptors",
        source,
    })
}

/// Stage the removal of every descriptor the forked-in tree carries that
/// `granted` does not cover.
///
/// Issues **no** git command when there is nothing to drop — the shipped
/// default, whose `worker` grant is the whole pool (§4.3), and equally a
/// fork off a parent tip already cut to the same grant. Idempotent for
/// that reason, and `--ignore-unmatch` keeps a tool whose skill
/// frontmatter never snapshotted from being a failure.
fn drop_ungranted(worktree: &Path, granted: &[String], git: &dyn GitRunner) -> Result<(), Error> {
    let mut paths: Vec<String> = Vec::new();
    for name in ungranted(worktree, granted)? {
        paths.push(schema_path(&name));
        paths.push(skill_path(&name));
    }
    if paths.is_empty() {
        return Ok(());
    }
    let mut args: Vec<&str> = vec!["rm", "-q", "--ignore-unmatch", "--"];
    args.extend(paths.iter().map(String::as_str));
    git.run(worktree, &args).map_err(|source| Error::Git {
        op: "rm ungranted descriptors",
        source,
    })
}

/// The tool names this tree carries a schema for that `granted` does not
/// list, sorted so the staged removal is deterministic.
///
/// A tree with no `descriptions/tools/` at all yields none: nothing was
/// snapshotted there, so nothing is stranded. That is the ordinary case
/// for a child forked off a parent tip with a narrow grant, and for the
/// stub-git unit fixtures.
fn ungranted(worktree: &Path, granted: &[String]) -> Result<Vec<String>, Error> {
    let entries = match std::fs::read_dir(worktree.join(TOOLS_DIR)) {
        Ok(iter) => iter,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(e.into()),
    };
    let mut names: Vec<String> = entries
        .flatten()
        .filter_map(|e| {
            let file = e.file_name().to_string_lossy().into_owned();
            file.strip_suffix(".json").map(str::to_owned)
        })
        .filter(|name| !granted.iter().any(|g| g == name))
        .collect();
    names.sort();
    Ok(names)
}

/// Does the config commit's tree carry `path`? (`git cat-file -e`.)
/// Shared with [`super::reviewer_read`], which asks the same question of
/// the same commit about its own two paths.
pub(super) fn committed(dir: &Path, commit: &str, path: &str, git: &dyn GitRunner) -> bool {
    git.run(dir, &["cat-file", "-e", &format!("{commit}:{path}")])
        .is_ok()
}

/// The tools the governing config commit *does* describe, rendered for a
/// decline ([`crate::name::pool`] — the "name the pool" idiom every
/// absent-name refusal shares). Read only when declining; a listing that
/// cannot be read renders as the empty pool, which is exactly what a
/// caller facing an undescribed grant must be told.
fn described_pool(dir: &Path, commit: &str, git: &dyn GitRunner) -> String {
    let spec = format!("{commit}:{TOOLS_DIR}");
    let listing = git
        .run_capture(dir, &["ls-tree", "--name-only", &spec])
        .unwrap_or_default();
    let names: Vec<&str> = listing
        .lines()
        .map(str::trim)
        .filter_map(|l| l.strip_suffix(".json"))
        .collect();
    crate::name::pool(&names)
}

/// `descriptions/tools/<tool>.json` — the schema half of a descriptor.
fn schema_path(tool: &str) -> String {
    format!("{TOOLS_DIR}/{tool}.json")
}

/// `descriptions/skills/<tool>.md` — the frontmatter half, when a tool
/// claims one.
fn skill_path(tool: &str) -> String {
    format!("{SKILLS_DIR}/{tool}.md")
}

mod refresh;
pub(crate) use refresh::refresh as refresh_descriptors;

#[cfg(test)]
mod tests;
