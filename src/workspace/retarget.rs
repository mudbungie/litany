//! The agent's **retarget mark** — `refs/litany/retarget/<agent-id>`
//! (ARCH §2.2 *Fork chooses the lineage*, §3.4 `litany retarget`).
//!
//! Fork chooses the lineage and resolution follows its tip (§2.2,
//! bl-403b), so a same-lineage config edit reaches the agent by
//! resolution alone. The **retarget mark** is the act that remains: a
//! user act naming the config commit — another lineage's head — the
//! agent should be governed by from its next step on, consumed by the
//! agent's **own executor** at the next `advance` step boundary
//! ([`crate::prompt::retarget`]).
//!
//! Writing a ref is what keeps §2.3's branch-advancement invariant intact:
//! the user marks, the executor lands. Nothing else writes the agent's
//! branch, and no second writer appears — the same shape every other
//! orthogonal, non-derivable per-agent fact takes ([`super::MARK_REF_ROOT`]
//! — `conflicted`, `budget-exhausted`, `abandoned`, `notify`, `cwd`), so
//! it is reaped with the agent by `litany delete` (§9.2 enumerates the
//! mark root) and crosses no fork and no transfer.
//!
//! **The mark names a commit, not a value.** `cwd` (§3.3) points at a
//! blob because its fact is a path; this one points at the target
//! **config commit** itself, which is exactly the fact — so the landing
//! reads a commit-ish and nothing decodes anything. `git gc` keeps the
//! commit alive for as long as the mark does, which is what makes a
//! marked-then-rewound config lineage still land.
//!
//! **The role mark is the second half of the same act** (bl-946c),
//! `refs/litany/role/<agent-id>`, written by `litany retarget --role`
//! and consumed by the same landing at the same boundary. It is a
//! *second ref* and not a field on the first because the two are
//! orthogonal facts of different shapes — which config lineage governs
//! (a commit), and which role the agent is (a name, so a blob, the
//! `cwd` shape). Either may be marked without the other, and the
//! landing takes each absent one to mean *unchanged*: the general path
//! with empty inputs. Both live here because one verb writes them and
//! one landing answers them; a role has no other mark and no other
//! reader, its permanent home being the dispatch commit subject
//! ([`crate::prompt::role`]) the landing re-mints.

use super::{MARK_REF_ROOT, repo_git};
use crate::template::GitRunner;
use std::io;
use std::path::Path;

/// Ref-namespace prefix for the retarget mark (§2.2).
pub const RETARGET_REF_PREFIX: &str = "retarget/";

/// `refs/litany/retarget/<agent-id>` — the mark ref for one agent.
pub fn retarget_ref(agent_id: &str) -> String {
    format!("{MARK_REF_ROOT}{RETARGET_REF_PREFIX}{agent_id}")
}

/// The config commit an agent is marked to be retargeted to, or `None`
/// when no mark is set — the ordinary state of every agent, not an error.
/// An unreadable mark reads the same way: a step never fails for want of
/// a mark it does not have.
pub fn read(workspace: &Path, agent_id: &str, git: &dyn GitRunner) -> Option<String> {
    let spec = format!("{}^{{commit}}", retarget_ref(agent_id));
    let out = git
        .run_capture(&repo_git(workspace), &["rev-parse", "--verify", &spec])
        .ok()?;
    let sha = out.trim();
    (!sha.is_empty()).then(|| sha.to_string())
}

/// Mark `agent_id` for retargeting onto `commit` — last write wins, so an
/// operator who changes their mind before the next step simply marks
/// again.
pub fn write(
    workspace: &Path,
    agent_id: &str,
    commit: &str,
    git: &dyn GitRunner,
) -> io::Result<()> {
    git.run(
        &repo_git(workspace),
        &["update-ref", &retarget_ref(agent_id), commit],
    )
}

/// Ref-namespace prefix for the role mark (§4.3, bl-946c).
pub const ROLE_REF_PREFIX: &str = "role/";

/// `refs/litany/role/<agent-id>` — the role mark ref for one agent.
pub fn role_ref(agent_id: &str) -> String {
    format!("{MARK_REF_ROOT}{ROLE_REF_PREFIX}{agent_id}")
}

/// The role an agent is marked to be settled on, or `None` when no role
/// mark is set — the ordinary state of every agent, and the landing's
/// reading of *keep the role the branch already committed*. An
/// unreadable mark reads the same way, for the reason [`read`] gives.
pub fn read_role(workspace: &Path, agent_id: &str, git: &dyn GitRunner) -> Option<String> {
    let out = git
        .run_capture(
            &repo_git(workspace),
            &["cat-file", "blob", &role_ref(agent_id)],
        )
        .ok()?;
    let role = out.trim();
    (!role.is_empty()).then(|| role.to_string())
}

/// Mark `agent_id` for settling onto `role` — last write wins, like
/// [`write`]. The name is stored as a blob the ref points at, the
/// value-carrying mark shape `cwd` established (§3.3); role names are
/// `providers.yaml` keys, so no round-trip guard is needed beyond the
/// trim [`read_role`] performs.
pub fn write_role(
    workspace: &Path,
    agent_id: &str,
    role: &str,
    git: &dyn GitRunner,
) -> io::Result<()> {
    let repo = repo_git(workspace);
    // `git hash-object` reads a file and the trait's methods carry no
    // stdin, so the value is staged beside the repo under a pid-unique
    // name and removed once hashed — the `cwd` mark's own recipe.
    let staged = repo.join(format!("role-mark.{}.tmp", std::process::id()));
    std::fs::write(&staged, role.as_bytes())?;
    let staged_str = staged.to_string_lossy().into_owned();
    let hashed = git.run_capture(&repo, &["hash-object", "-w", "--", &staged_str]);
    std::fs::remove_file(&staged)?;
    git.run(&repo, &["update-ref", &role_ref(agent_id), &hashed?])
}

/// Consume the role mark, on every path out of the landing — the reason
/// [`clear`] gives, for the same boundary.
pub fn clear_role(workspace: &Path, agent_id: &str, git: &dyn GitRunner) -> io::Result<()> {
    git.run(
        &repo_git(workspace),
        &["update-ref", "-d", &role_ref(agent_id)],
    )
}

/// Consume the mark. Called by the executor once the landing has been
/// adjudicated — landed, declined, or a no-op alike — because in every
/// case the mark has been answered, and a surviving one would re-ask the
/// same question at every subsequent step boundary.
pub fn clear(workspace: &Path, agent_id: &str, git: &dyn GitRunner) -> io::Result<()> {
    git.run(
        &repo_git(workspace),
        &["update-ref", "-d", &retarget_ref(agent_id)],
    )
}

#[cfg(test)]
mod tests;
