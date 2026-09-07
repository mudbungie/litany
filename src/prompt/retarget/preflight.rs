//! The **pre-flight** half of the retarget (ARCH §2.2, §3.4): everything
//! `litany retarget` refuses before either mark is written, and the
//! [`Marks`] answer naming which of the two the verb then writes. Split
//! from the module root to hold the per-file line cap; the landing that
//! consumes those marks is the root's ([`super::land`]), and the two
//! shared derivations — the followed commit and the branch's committed
//! role — live there because both halves ask them.

use super::{Error, base, committed_role, dispatch, fork_point, governing, role};
use crate::template::GitRunner;
use crate::workspace;
use std::path::Path;

/// Why a retarget needed the agent to exist, for the shared
/// [`workspace::require_agent`] decline (§2.3).
const REASON: &str = "a retarget re-forks a running agent off another config commit (ARCH §2.2)";

/// What a retarget still has to change, and so which marks the verb
/// writes: the target config commit and the role, each `None` when the
/// agent is already in that state. Both `None` is the clean no-op — the
/// operator asked for a state the branch is already in.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Marks {
    /// The config commit to mark, `None` when it already governs.
    pub commit: Option<String>,
    /// The role to settle on, `None` when the branch already carries it.
    pub role: Option<String>,
}

impl Marks {
    /// Nothing to do — the state asked for is the state held.
    pub fn is_noop(&self) -> bool {
        self.commit.is_none() && self.role.is_none()
    }
}

/// Everything `litany retarget` refuses **before** either mark is
/// written (§3.4), returning what is left to change ([`Marks`]). Nothing
/// here writes, so a refusal leaves no debris at all: the same
/// validity-before-fork discipline the §6 budget gate and the §3.3
/// descriptor check hold to at every fork.
///
/// The checks are the ones a fork would run anyway, asked of the target:
/// the workspace and the agent exist, the config lineage exists, the
/// role the agent will resolve as is one the target config declares and
/// souls ([`role::validate::against`], asked only when `role` names a
/// new one — the role the branch already carries was validated at its
/// own fork), and that role is granted only tools the target config
/// describes ([`dispatch::require_described`]). What is deliberately
/// *not* checked is the tree: a retarget never inspects what the branch
/// has been doing, because the freeze it lifts is about policy.
pub fn preflight(
    workspace_dir: &Path,
    agent_id: &str,
    config_name: &str,
    role: Option<&str>,
    git: &dyn GitRunner,
) -> Result<Marks, Error> {
    workspace::require(workspace_dir)?;
    workspace::require_agent(workspace_dir, agent_id, REASON, git)?;
    workspace::require_lineage(workspace_dir, config_name, git)
        .map_err(|e| Error::from(fork_point::Error::from(e)))?;
    let repo = workspace::repo_git(workspace_dir);
    let branch = workspace::agent_ref(agent_id);
    let spec = format!("{}^{{commit}}", workspace::config_ref(config_name));
    let target = git
        .run_capture(&repo, &["rev-parse", &spec])
        .map_err(|source| Error::Git {
            op: "retarget resolve target",
            source,
        })?
        .trim()
        .to_string();
    let held = committed_role(&repo, agent_id, &branch, git)?;
    let asked = role.unwrap_or(&held);
    let marks = Marks {
        commit: (governing(workspace_dir, &branch, git)? != target).then(|| target.clone()),
        role: (asked != held).then(|| asked.to_string()),
    };
    if marks.is_noop() {
        return Ok(marks);
    }
    if marks.role.is_some() {
        role::validate::against(
            workspace_dir,
            &target,
            &format!("agent {agent_id:?}"),
            asked,
            git,
        )?;
    }
    let tools = base::granted(workspace_dir, &target, asked, git)?;
    dispatch::require_described(
        &repo,
        &dispatch::Grant {
            role: asked,
            tools: &tools,
            config_commit: &target,
        },
        git,
    )?;
    Ok(marks)
}
