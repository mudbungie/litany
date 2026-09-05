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

/// The agent's workflow (§6) **and the commit it was read from**: the
/// nearest workflow mark's commit when one stands, else the governing
/// config commit — the general path. A marked commit gets its own §10
/// schema-version guard before its workflow is interpreted, exactly as
/// the governing commit did in [`super::resolve_worker`]: a mark aimed
/// at a commit authored by a newer harness declines before parsing
/// shapes it cannot read.
///
/// The commit comes back rather than being re-derived by whoever wants
/// it: the mark is a ref an operator may rewrite at any moment, so a
/// second read of it after the step is a different question with a
/// different answer. The step record's provenance (bl-e4a0) is what
/// *this* resolution answered.
pub(super) fn resolve_workflow(
    workspace: &Path,
    source: &ConfigSource<'_>,
    governing: &str,
    deps: &Deps<'_>,
) -> Result<(Workflow, String), Error> {
    let answer = source_of(workspace, source, governing, deps.git);
    if let Source::Marked { commit, .. } = &answer {
        let version_raw = read_control(workspace, commit, VERSION_FILE, deps)?;
        Version::parse(&version_raw, &control_origin(commit, VERSION_FILE))?;
    }
    let commit = answer.commit().to_string();
    let workflow = read_workflow(workspace, &commit, deps)?;
    Ok((workflow, commit))
}

/// **Which commit answers the workflow question for one agent, and
/// why** — the derivation above as a value, so the resolver and the
/// operator read (`litany workflow <ws> <agent>`, bl-5c02) compose it
/// once instead of each spelling out "nearest mark, else the followed
/// commit". Without this the only way to ask was raw git against a ref
/// namespace no verb printed.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Source {
    /// A standing mark on the agent's descent answers. `holder` is the
    /// id carrying it — the agent's own, or the ancestor whose mark it
    /// inherits, which is the fact an operator cannot guess: marking a
    /// root switches its whole tree (§6).
    Marked { holder: String, commit: String },
    /// No mark anywhere on the descent, so the followed config commit
    /// answers — the general path every unmarked agent takes.
    Followed { commit: String },
}

impl Source {
    /// The commit whose `workflow.yaml` governs, whichever arm answered.
    pub(crate) fn commit(&self) -> &str {
        match self {
            Source::Marked { commit, .. } | Source::Followed { commit } => commit,
        }
    }
}

/// Compose the derivation: the nearest mark on the descent, else
/// `followed`. A [`ConfigSource::Fork`] is a root that has no id yet and
/// therefore no mark — the general path with empty inputs, not a case.
pub(crate) fn source_of(
    workspace: &Path,
    source: &ConfigSource<'_>,
    followed: &str,
    git: &dyn GitRunner,
) -> Source {
    let marked = match source {
        ConfigSource::Fork(_) => None,
        ConfigSource::Agent(agent_id) => nearest_mark(workspace, agent_id, git),
    };
    match marked {
        Some((holder, commit)) => Source::Marked { holder, commit },
        None => Source::Followed {
            commit: followed.to_string(),
        },
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
/// The answer is the **holder and the commit**, not the commit alone:
/// which id on the descent carries the mark is the half an operator
/// cannot derive from the agent they asked about, and it is what makes
/// "marking a root switches the tree" legible on the read surface.
pub(in crate::prompt) fn nearest_mark(
    workspace: &Path,
    agent_id: &str,
    git: &dyn GitRunner,
) -> Option<(String, String)> {
    let mut id = agent_id.to_string();
    loop {
        if let Some(commit) = workflow_mark::read(workspace, &id, git) {
            return Some((id, commit));
        }
        id = crate::prompt::inbox::parent_of(&id)?;
    }
}
