//! The agent's **workflow mark** — `refs/litany/workflow/<agent-id>`
//! (ARCH §6 *The workflow mark*, `docs/DESIGN_WORKFLOW_SWITCH.md`).
//!
//! The workflow is the named what-happens-next policy — the config's
//! `workflow.yaml` (§6) — and by default it follows the governing
//! lineage's current tip like every other control fact (§2.2,
//! bl-403b). The mark is the workflow fact's per-agent override
//! (operator ruling 2026-08-31): a **standing** ref naming the config
//! commit whose `workflow.yaml` governs the agent from its next step
//! boundary on — winning over the followed tip, which is exactly what
//! makes it a deliberate pin as well as a switch. Resolution consults it fresh at every hop
//! ([`crate::prompt`]'s `resolve::workflow_source` — nearest mark on the
//! agent's descent, else the governing config commit), so writing the
//! ref *is* the switch, effective at the next step, with no landing, no
//! rebase and no migration anywhere.
//!
//! Unlike the retarget mark ([`super::retarget`]) this one is **not
//! consumed**: it is standing policy, the same class of non-derivable
//! per-agent assertion as `abandoned` (§6), persisting until re-marked
//! or cleared. It stores no workflow content — content lives only in
//! config commits (§2.2) — and it lives in the shared mark namespace
//! ([`super::MARK_REF_ROOT`]) so the retention delete reaps it with the
//! agent (§9.2) and an archive carries it with the refs.
//!
//! **The mark names a commit, not a lineage.** The verb hands it
//! `config/<name>`'s head resolved to a commit, so a lineage that
//! advances after the mark changes nothing the agent reads — the same
//! immutability the config freeze buys, chosen instead of inherited.

use super::{MARK_REF_ROOT, repo_git};
use crate::template::GitRunner;
use std::io;
use std::path::Path;

/// Ref-namespace prefix for the workflow mark (§6).
pub const WORKFLOW_REF_PREFIX: &str = "workflow/";

/// `refs/litany/workflow/<agent-id>` — the mark ref for one agent.
pub fn workflow_ref(agent_id: &str) -> String {
    format!("{MARK_REF_ROOT}{WORKFLOW_REF_PREFIX}{agent_id}")
}

/// The config commit whose `workflow.yaml` this agent is marked to run
/// under, or `None` when no mark is set — the ordinary state of every
/// agent, not an error. An unreadable mark reads the same way: a step
/// never fails for want of a mark it does not have.
pub fn read(workspace: &Path, agent_id: &str, git: &dyn GitRunner) -> Option<String> {
    let spec = format!("{}^{{commit}}", workflow_ref(agent_id));
    let out = git
        .run_capture(&repo_git(workspace), &["rev-parse", "--verify", &spec])
        .ok()?;
    let sha = out.trim();
    (!sha.is_empty()).then(|| sha.to_string())
}

/// Mark `agent_id` to run under `commit`'s `workflow.yaml` — last write
/// wins, so switching again is just marking again.
pub fn write(
    workspace: &Path,
    agent_id: &str,
    commit: &str,
    git: &dyn GitRunner,
) -> io::Result<()> {
    git.run(
        &repo_git(workspace),
        &["update-ref", &workflow_ref(agent_id), commit],
    )
}

/// Delete the mark: from the agent's next step boundary on, the
/// governing config commit's `workflow.yaml` governs again — removing
/// the mark deletes config, never code (`docs/PRINCIPLES.md`
/// severability).
pub fn clear(workspace: &Path, agent_id: &str, git: &dyn GitRunner) -> io::Result<()> {
    git.run(
        &repo_git(workspace),
        &["update-ref", "-d", &workflow_ref(agent_id)],
    )
}

#[cfg(test)]
mod tests;
