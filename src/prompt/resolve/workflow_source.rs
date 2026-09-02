//! Which commit answers the **workflow** question (ARCH §6 *The workflow
//! mark*, `docs/DESIGN_WORKFLOW_SWITCH.md`).
//!
//! The workflow — the named what-happens-next policy, `workflow.yaml`
//! (§6) — is the one control fact with a per-agent override: a standing
//! **workflow mark** ([`crate::workspace::workflow_mark`]) names the
//! config commit whose `workflow.yaml` governs the agent instead of
//! the followed config commit's (§2.2 follow-the-tip, bl-403b — so the
//! mark both switches the workflow and pins it against tip movement). Because resolution runs fresh at every hop (§6 "no resident
//! interpreter"), consulting the mark here makes the switch effective at
//! the agent's next step boundary with no landing machinery at all —
//! writing the ref *is* the switch.
//!
//! The derivation is **nearest mark on the agent's descent**: the
//! agent's own id first, then each ancestor by
//! [`crate::prompt::inbox::parent_of`] — so marking a root switches its
//! whole tree, and a child's own mark overrides its ancestors', the same
//! nearest-wins shape as governing-config ancestry (§2.2). A fresh root
//! about to fork ([`ConfigSource::Fork`]) has no id yet and so no mark:
//! the followed commit answers, which is the whole path every unmarked
//! agent takes.

use super::{ConfigSource, VERSION_FILE, control_origin, read_control};
use crate::config::Workflow;
use crate::config::version::Version;
use crate::prompt::{Deps, Error, WORKFLOW_FILE};
use crate::template::GitRunner;
use crate::workspace::workflow_mark;
use std::path::Path;

/// The agent's workflow (§6): the nearest workflow mark's commit when
/// one stands, else the governing config commit — the general path. A
/// marked commit gets its own §10 schema-version guard before its
/// workflow is interpreted, exactly as the governing commit did in
/// [`super::resolve_worker`]: a mark aimed at a commit authored by a
/// newer harness declines before parsing shapes it cannot read.
pub(super) fn resolve_workflow(
    workspace: &Path,
    source: &ConfigSource<'_>,
    governing: &str,
    deps: &Deps<'_>,
) -> Result<Workflow, Error> {
    let marked = match source {
        ConfigSource::Fork(_) => None,
        ConfigSource::Agent(agent_id) => nearest_mark(workspace, agent_id, deps.git),
    };
    match marked {
        None => read_workflow(workspace, governing, deps),
        Some(commit) => {
            let version_raw = read_control(workspace, &commit, VERSION_FILE, deps)?;
            Version::parse(&version_raw, &control_origin(&commit, VERSION_FILE))?;
            read_workflow(workspace, &commit, deps)
        }
    }
}

/// One workflow read: `<commit>:workflow.yaml`, parsed under the closed
/// §6 vocabulary. The same read whichever commit answers — marked or
/// governing — so no second code path exists to drift.
fn read_workflow(workspace: &Path, commit: &str, deps: &Deps<'_>) -> Result<Workflow, Error> {
    let raw = read_control(workspace, commit, WORKFLOW_FILE, deps)?;
    Ok(Workflow::parse(
        &raw,
        &control_origin(commit, WORKFLOW_FILE),
    )?)
}

/// The nearest workflow mark on `agent_id`'s descent — its own, else
/// the walk up `parent_of` to the root. `None` — no mark anywhere on
/// the chain — is the ordinary state of every agent. Shared with the §6
/// dispatch budget gate ([`crate::prompt::child_dispatch`]), so the
/// ceiling a fork is refused under and the ceiling the child's own
/// steps check are one answer, not two.
pub(in crate::prompt) fn nearest_mark(
    workspace: &Path,
    agent_id: &str,
    git: &dyn GitRunner,
) -> Option<String> {
    let mut id = agent_id.to_string();
    loop {
        if let Some(commit) = workflow_mark::read(workspace, &id, git) {
            return Some(commit);
        }
        id = crate::prompt::inbox::parent_of(&id)?;
    }
}
