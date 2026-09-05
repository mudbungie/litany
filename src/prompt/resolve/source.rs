//! The resolution *source* half of [`super`] (ARCH §2.2, §6): which
//! agent or fork point is being resolved, the role it resolves as, and
//! the **followed config commit** its control is read from (§2.2 *Fork
//! chooses the lineage*, bl-403b). Split from the module root to hold
//! the per-file line cap; every consumer path is unchanged
//! (`ConfigSource` is re-exported there).

use crate::prompt::notice::notice;
use crate::prompt::{Deps, Error, WORKER_ROLE};
use crate::workspace;
use std::path::Path;

/// Which config commit governs the resolution (ARCH §2.2).
pub(crate) enum ConfigSource<'a> {
    /// A fresh root about to fork off this ref (§2.3 *Any ref is a legal
    /// fork point*): a config lineage's head, or any commit of any agent
    /// (`--from`, §7.2). Either way resolution follows the governing
    /// lineage of that ref to its current tip (§2.2, bl-403b) — fork
    /// chooses the lineage, never the moment.
    Fork(&'a str),
    /// An existing agent: the current tip of its branch's governing
    /// lineage (§2.2, bl-403b), derived from ancestry plus the refs.
    Agent(&'a str),
}

/// The agent's role (§6 role-aware resolution). A fresh root about to
/// fork has no dispatch commit yet, so it is the worker default; an
/// existing agent's role is derived from its own dispatch commit subject
/// — the single authoritative home ([`crate::prompt::role`]) — falling
/// back to the worker default for a root branch (whose subject lacks the
/// `dispatch: <role>` prefix).
pub(super) fn agent_role(
    workspace: &Path,
    source: &ConfigSource<'_>,
    deps: &Deps<'_>,
) -> Result<String, Error> {
    match source {
        ConfigSource::Fork(_) => Ok(WORKER_ROLE.to_string()),
        ConfigSource::Agent(agent_id) => Ok(crate::prompt::role::derive(
            &workspace::repo_git(workspace),
            &workspace::agent_ref(agent_id),
            agent_id,
            deps.git,
        )?
        .unwrap_or_else(|| WORKER_ROLE.to_string())),
    }
}

/// Resolve the **followed** config commit sha for the source (§2.2
/// *Fork chooses the lineage*, bl-403b) — one derivation for both,
/// asked of the fork point for a fresh root and of its own ref for an
/// existing agent ([`workspace::current_config`]): the governing
/// lineage's current tip, so a config edit reaches every conversation
/// at its next step boundary. Diverged lineages cannot be followed —
/// control resolves the fork commit, and this says so loudly (the
/// notice is the surface the pinned-to-a-dead-config incident lacked);
/// `litany retarget` settles the lineage.
pub(super) fn config_commit(
    workspace: &Path,
    source: &ConfigSource<'_>,
    deps: &Deps<'_>,
) -> Result<String, Error> {
    let rev = match source {
        ConfigSource::Fork(fork_point) => (*fork_point).to_owned(),
        ConfigSource::Agent(agent_id) => workspace::agent_ref(agent_id),
    };
    let resolved =
        workspace::current_config::current_config(workspace, &rev, deps.git).map_err(|source| {
            Error::Git {
                op: "followed config",
                source,
            }
        })?;
    if let Some(tips) = resolved.held() {
        notice!(
            "{tips} diverged config lineages reach [{rev}] — control resolves its fork \
             commit until `litany retarget` settles the lineage (ARCH §2.2)",
        );
    }
    Ok(resolved.commit().to_string())
}
