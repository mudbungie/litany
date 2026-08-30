//! The **re-derived dispatch commit** (ARCH §2.2): the base a retarget
//! landing replays the agent's own history onto.
//!
//! It is not the old dispatch commit rebased — it is a *fresh* one, minted
//! through the fork's own machinery
//! ([`crate::prompt::dispatch::trim_to_context`]) against the **target**
//! config commit, and parented on it. Three things fall out of that,
//! rather than being arranged:
//!
//! - **The §3.3 descriptor cut is re-derived, never replayed** — which is
//!   what §3.3 already requires of every fork ("the derivation reads that
//!   commit and never the tree it forked off"). Minting a dispatch commit
//!   *is* that derivation, so the retarget writes nothing special for it,
//!   and a grant the target config does not describe is declined here by
//!   the same check every fork runs.
//! - **The control-file removal is re-performed** against the target, so
//!   the modify/delete a naive rebase would hit on `providers.yaml` — the
//!   new parent carries it, the old dispatch commit deletes it — never
//!   arises. The base already has it gone.
//! - **`goal.md`, `soul.md` and `name` are re-pinned from the sources the
//!   original fork used** (§2.3 *Goal and soul are pinned files*): goal
//!   and name are the agent's own and ride the old dispatch tree
//!   untouched; the soul is re-read from the target's `souls/<role>.md`,
//!   which is the whole point of retargeting a role whose soul moved.
//!
//! **The subject is the old one, verbatim.** A branch's founding commit is
//! identified by its subject ([`role::founding_sha`]), and the checkpoint
//! clock (§2.7) and role derivation (§6) both read it — so re-minting it
//! under any other wording would move the branch's own founding out from
//! under them. The commit is new; what it *says* is the fact it restates.
//!
//! The mint never disturbs the live checkout: a throwaway detached
//! worktree at the old dispatch commit gives the trim a tree to work in,
//! and the result leaves as `write-tree` + `commit-tree`. Pure object-store
//! writes; the branch ref moves in the replay, not here
//! ([`crate::prompt::rebase_forward`]).

use crate::config::PerRepoProviders;
use crate::prompt::{Error, dispatch, subagent};
use crate::template::GitRunner;
use crate::workspace::{self, agent_name};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

/// Mint the re-derived dispatch commit for `agent_id` on top of the
/// target config commit (`grant.config_commit`) and return its sha.
/// `worktree` is the agent's live checkout, used only as an access point
/// onto the workspace's object store (§2.2) — its tree is never touched.
pub(super) fn commit(
    workspace_dir: &Path,
    worktree: &Path,
    agent_id: &str,
    dispatch_sha: &str,
    grant: &dispatch::Grant<'_>,
    git: &dyn GitRunner,
) -> Result<String, Error> {
    let target = grant.config_commit;
    let soul_rel = format!("{}/{}.md", crate::prompt::SOULS_DIR, grant.role);
    let soul =
        workspace::show_control(workspace_dir, target, &soul_rel, git).map_err(|source| {
            Error::ControlRead {
                path: PathBuf::from(format!("{target}:{soul_rel}")),
                source,
            }
        })?;

    let tmp = scratch_worktree(agent_id);
    let tmp_str = tmp.to_string_lossy().into_owned();
    git.run(
        worktree,
        &["worktree", "add", "--detach", &tmp_str, dispatch_sha],
    )
    .map_err(|source| Error::Git {
        op: "retarget scratch worktree",
        source,
    })?;
    let minted = mint(&tmp, worktree, dispatch_sha, grant, &soul, git);
    // The scratch worktree is disposable either way; a removal failure
    // must not shadow the mint's own outcome.
    let _ = git.run(worktree, &["worktree", "remove", "--force", &tmp_str]);
    minted
}

/// The role's `tools:` grant as the **target** config commit declares it
/// (§4.3). A role the target does not list grants none — §4.3's own
/// reading of an omitted list, and the compactor's shape — so a config
/// that dropped the role retargets to an empty toolset rather than
/// failing; what it may *not* do is grant a tool the same commit does not
/// describe, which the pre-flight declines (§3.3).
pub(super) fn granted(
    workspace_dir: &Path,
    target: &str,
    role: &str,
    git: &dyn GitRunner,
) -> Result<Vec<String>, Error> {
    let file = crate::prompt::PER_REPO_PROVIDERS_FILE;
    let raw = workspace::show_control(workspace_dir, target, file, git).map_err(|source| {
        Error::ControlRead {
            path: PathBuf::from(format!("{target}:{file}")),
            source,
        }
    })?;
    let providers = PerRepoProviders::parse(&raw, Path::new(&format!("{target}:{file}")))?;
    Ok(providers
        .roles
        .get(role)
        .map(|assignment| assignment.tools.clone())
        .unwrap_or_default())
}

/// The object-store half of [`commit`], run inside the scratch worktree:
/// the fork's own trim against the target config commit, the re-pinned
/// soul, then the tree and the commit. `access` is any checkout onto the
/// same object store — the subject is read there because the scratch
/// worktree is about to be torn down.
fn mint(
    tmp: &Path,
    access: &Path,
    dispatch_sha: &str,
    grant: &dispatch::Grant<'_>,
    soul: &str,
    git: &dyn GitRunner,
) -> Result<String, Error> {
    let err = |op| move |source| Error::Git { op, source };
    // The name is the agent's own committed fact and is carried across
    // unchanged (§2.3): the trim re-settles what the old dispatch commit
    // already wrote, which is a rewrite of identical bytes.
    let name = agent_name::in_worktree(tmp);
    dispatch::trim_to_context(tmp, grant, name.as_deref(), git)?;
    std::fs::write(tmp.join(subagent::SOUL_FILE), soul)?;
    git.run(tmp, &["add", "-A"]).map_err(err("retarget add"))?;
    let tree = git
        .run_capture(tmp, &["write-tree"])
        .map_err(err("retarget write-tree"))?;
    let subject = git
        .run_capture(access, &["log", "-n", "1", "--format=%s", dispatch_sha])
        .map_err(err("retarget dispatch subject"))?;
    let sha = git
        .run_capture(
            tmp,
            &[
                "commit-tree",
                tree.trim(),
                "-p",
                grant.config_commit,
                "-m",
                subject.trim(),
            ],
        )
        .map_err(err("retarget commit-tree"))?;
    Ok(sha.trim().to_string())
}

/// A unique scratch-worktree path outside every worktree, keyed by the
/// agent id and a nanosecond stamp (the same shape the compaction base and
/// the transfer's patch path use, §2.6).
fn scratch_worktree(agent_id: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    std::env::temp_dir().join(format!("litany-retarget-base-{agent_id}-{nanos}"))
}
