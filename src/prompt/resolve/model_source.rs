//! Which binding answers the **model** question (ARCH §4.3 *The model
//! mark*, bl-246f).
//!
//! The role's `roles:` entry on the followed config commit binds the
//! model for every unmarked agent (§4.3) — the general path. A standing
//! **model mark** ([`crate::workspace::model_mark`]) overrides exactly the
//! fields it carries — `provider`, `model`, `effort`, `priority` — for the
//! one agent it names; `tools` and `max_output_tokens` stay the
//! assignment's, so the overlay is a value the resolver reads and never
//! a second place a grant could come from. Because resolution runs fresh
//! at every hop, consulting the mark here makes a pick effective at the
//! agent's next step boundary with no landing machinery — writing the
//! ref *is* the switch, the workflow mark's own rule
//! ([`super::workflow_source`]).
//!
//! **No descent walk.** A fresh root about to fork ([`ConfigSource::Fork`])
//! has no id and so no mark; an existing agent is asked about by its own
//! id and nothing else. A pick is about the conversation in front of the
//! operator, and a child's role has its own entry (§4.3).

use super::ConfigSource;
use crate::config::PerRepoProviders;
use crate::config::per_repo_providers::RoleAssignment;
use crate::prompt::Error;
use crate::template::GitRunner;
use crate::workspace::model_mark::{self, ModelMark};
use std::path::Path;

/// The role's assignment as this agent's next step will run it, and the
/// mark that overlaid it when one stood — carried, never re-derived, so
/// the step record's provenance (`meta.json` `model_mark`, §2.3) is what
/// *this* resolution answered rather than what the ref says later.
pub(super) struct Binding {
    pub(super) assignment: RoleAssignment,
    pub(super) mark: Option<ModelMark>,
}

/// Compose the derivation: the role's entry off the followed commit's
/// `providers.yaml`, overlaid by the agent's standing mark when one
/// stands. A role the config does not declare is still the config's
/// refusal — the mark overlays an assignment and cannot stand in for a
/// missing one, so a pick never makes an undeclared role dispatchable.
pub(super) fn resolve_binding(
    workspace: &Path,
    source: &ConfigSource<'_>,
    role: &str,
    per_repo: &PerRepoProviders,
    git: &dyn GitRunner,
) -> Result<Binding, Error> {
    let mut assignment = per_repo
        .roles
        .get(role)
        .cloned()
        .ok_or_else(|| Error::RoleMissing(role.to_owned()))?;
    let mark = match source {
        ConfigSource::Fork { .. } => None,
        ConfigSource::Agent(agent_id) => model_mark::read(workspace, agent_id, git)?,
    };
    if let Some(mark) = &mark {
        overlay(&mut assignment, mark);
    }
    Ok(Binding { assignment, mark })
}

/// The overlay, in one place: the four fields a mark carries replace the
/// assignment's, the rest stand. `effort` and `priority` are replaced
/// *whole* — an absent knob on the mark means none requested, as it does
/// on the assignment, and not "keep the lineage's" (§4.3; the mark is a
/// complete binding, read without reference to what it replaced).
pub(super) fn overlay(assignment: &mut RoleAssignment, mark: &ModelMark) {
    assignment.provider = mark.provider.clone();
    assignment.model = mark.model.clone();
    assignment.effort = mark.effort;
    assignment.priority = mark.priority;
}

#[cfg(test)]
mod tests;
