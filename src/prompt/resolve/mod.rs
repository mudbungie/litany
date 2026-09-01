//! Worker-role config resolution shared by every step-driving verb
//! (ARCH §2.2, §4.2, §4.3, §6).
//!
//! Control is read from a **config commit** (ARCH §2.2), never from a
//! worktree file: `litany prompt` (a fresh root) resolves against the
//! **governing config commit of the ref it is about to fork off** — the
//! config commit itself for the ordinary fresh start, its nearest
//! `config/*` ancestor when the start forks from history (§2.3, §7.2) —
//! and `litany advance` (the §6 hop) resolves the governing config
//! commit of the existing agent's branch. Both are the one ancestry
//! derivation ([`crate::workspace::governing_config`]). The reads —
//! `providers.yaml`, `workflow.yaml` policy, the role soul — go through
//! `git show <commit>:<path>`; the global `models.yaml` and the adapter
//! binary with its load-time version guard (§4.4) resolve as before.
//! [`WorkerConfig`] is the owned product; [`WorkerConfig::as_resolved`]
//! borrows it into the [`dispatch::Resolved`] shape the step machinery
//! consumes. `litany advance` resolves *lazily*: a no-op hop (lost
//! acquire, nothing due) exits before any config is read (§6).

pub(in crate::prompt) mod workflow_source;

#[cfg(test)]
mod tests;

use super::{Deps, Error, GLOBAL_MODELS_FILE, PER_REPO_PROVIDERS_FILE, SOULS_DIR, WORKER_ROLE};
use crate::config::manifest::{Manifest, RoleRules};
use crate::config::version::Version;
use crate::config::{ModelsConfig, Workflow, cross};
use crate::prompt::{adapter, dispatch};
use crate::workspace;
use std::ffi::OsString;
use std::path::{Path, PathBuf};

/// The config's schema version (ARCH §10), read from the config commit
/// (§2.2) like every other control file.
const VERSION_FILE: &str = "version";

/// Control file declaring the per-role context-assembly rules (ARCH
/// §5.2), read from the config commit (§2.2).
const MANIFEST_FILE: &str = "manifest.yaml";

/// Which config commit governs the resolution (ARCH §2.2).
pub(super) enum ConfigSource<'a> {
    /// A fresh root about to fork off this ref (§2.3 *Any ref is a legal
    /// fork point*): a config lineage's head, or any commit of any agent
    /// (`--from`, §7.2). Either way the governing config commit is the
    /// nearest `config/*` ancestor of that ref — a config head answers
    /// itself — so the fork is the freeze whatever it forks off.
    Fork(&'a str),
    /// An existing agent: its governing config commit, the nearest
    /// `config/*` ancestor of its branch (§2.2), derived from ancestry.
    Agent(&'a str),
}

/// The owned resolution of the worker role against one workspace:
/// everything a step needs that is not on the branch itself. Owned (not
/// borrowed from a `ModelsConfig`) so callers that resolve lazily —
/// `litany advance` — can return it from the resolving scope.
#[derive(Clone)]
pub(super) struct WorkerConfig {
    /// The agent's role (ARCH §2.5, §4.3), derived from its dispatch
    /// commit subject — the single authoritative home ([`crate::prompt::
    /// role`]). A fresh root resolves as `worker`; a dispatched child
    /// resolves the role its parent pinned. Governs which `souls/<role>.md`
    /// and `providers.yaml` role assignment were read, and whether the
    /// built-in compactor toolset is injected (§2.7, the step composes it
    /// for the `compactor` role alone).
    pub(super) role: String,
    /// The model id the role's `providers.yaml` assignment names (§4.3)
    /// — the single home of the pointer; it rides the canonical request
    /// verbatim, and its validity is brazen's fact, caught at the first
    /// live model call (§4.2).
    pub(super) model_id: String,
    /// brazen provider-row name passed as `bz --provider <row>` (§4.4).
    pub(super) provider_row: String,
    /// The role's declared tool names (§4.3 `tools:`).
    pub(super) tools: Vec<String>,
    /// The config commit every control file above was read from (§2.2) —
    /// a config branch's head for a fresh root, the ancestry derivation
    /// for an existing agent. Carried because the dispatch commit derives
    /// the agent's `descriptions/**` from it (§3.3), and it must be the
    /// *same* commit the grant came from.
    pub(super) config_commit: String,
    pub(super) soul: String,
    /// The adapter binary (`bz` or the `adapter:` override, §4.2).
    pub(super) binary: OsString,
    /// The agent's workflow — the one control fact with its own source
    /// derivation (§6 *The workflow mark*, [`workflow_source`]): the
    /// nearest workflow mark's commit when one stands, else the
    /// governing config commit (§2.2), which is every unmarked agent's
    /// path. Carries the event→action bindings the §6 interpreter runs, and
    /// is the single home for the retry policy and budgets — `as_resolved`
    /// derives both from it rather than mirroring them into their own
    /// fields (`docs/PRINCIPLES.md` Single source of truth).
    pub(super) workflow: Workflow,
    /// The role's context-assembly rules from the config's
    /// `manifest.yaml` `roles:` map (§5.2), read from the same config
    /// commit. `None` when the manifest lists no entry for this role —
    /// assembly then composes the transcript alone, the general path
    /// with empty inputs (a manifest role entry is not part of role
    /// validity, §4.3).
    pub(super) manifest: Option<RoleRules>,
    /// True under a named adapter target — a `models.yaml` `adapter:`
    /// override or a binding-injected host target (`cmd::Fx::adapter_target`,
    /// §3.4) — where the MessageStart.v handshake governs in place of the
    /// version guard (§4.4).
    pub(super) expect_handshake: bool,
}

impl WorkerConfig {
    /// Borrow into the [`dispatch::Resolved`] shape the step machinery
    /// takes (one struct, two drivers — §6 shipped-state note). Retry and
    /// budgets derive from the one `workflow` home (§6).
    pub(super) fn as_resolved(&self) -> dispatch::Resolved<'_> {
        dispatch::Resolved {
            grant: dispatch::Grant {
                role: &self.role,
                tools: &self.tools,
                config_commit: &self.config_commit,
            },
            model_id: &self.model_id,
            provider_row: &self.provider_row,
            soul: self.soul.clone(),
            binary: self.binary.clone(),
            retry: self.workflow.retry,
            budgets: self.workflow.budgets,
            workflow: &self.workflow,
            manifest: self.manifest.as_ref(),
            expect_handshake: self.expect_handshake,
        }
    }
}

/// Resolve the worker role against `workspace`: derive the config
/// commit, read the control files from its tree, run the load-time
/// version guard (§4.4), and read the role soul.
pub(super) fn resolve_worker(
    workspace: &Path,
    source: ConfigSource<'_>,
    deps: &Deps<'_>,
) -> Result<WorkerConfig, Error> {
    let commit = config_commit(workspace, &source, deps)?;
    // §6 role-aware resolution: an agent's role is derived from its
    // dispatch commit subject — the single authoritative home
    // (`crate::prompt::role`). A fresh root has no agent branch yet and no
    // dispatch commit to read, so it resolves the worker default; an
    // existing agent (the `litany advance` hop) reads its own subject, so a
    // dispatched compactor resolves `souls/compactor.md` and the compactor
    // `providers.yaml` assignment rather than the worker's.
    let role = agent_role(workspace, &source, deps)?;

    // §10 schema-version guard, first of the control reads: a config
    // commit authored by a newer harness may carry shapes the parsers
    // below cannot read, so decline before interpreting any of them.
    let version_raw = read_control(workspace, &commit, VERSION_FILE, deps)?;
    Version::parse(&version_raw, &control_origin(&commit, VERSION_FILE))?;

    let global_path = deps.config_root.join(GLOBAL_MODELS_FILE);
    let providers_raw = read_control(workspace, &commit, PER_REPO_PROVIDERS_FILE, deps)?;
    let cfg = ModelsConfig::load_with_per_repo(
        &global_path,
        &providers_raw,
        &control_origin(&commit, PER_REPO_PROVIDERS_FILE),
    )?;

    // The role assignment is the whole model binding (§4.3): the
    // provider row name goes to `bz --provider`, the model id rides the
    // canonical request verbatim. Id validity is brazen's fact, caught
    // at the first live model call (§4.2) — no global table mediates.
    let assignment = cfg
        .per_repo
        .roles
        .get(role.as_str())
        .ok_or_else(|| Error::RoleMissing(role.clone()))?;

    // Adapter resolution (§4.2/§4.4), one order (most-specific first): the
    // optional `models.yaml` `adapter:` override, else the binding-injected
    // host target (`cmd::Fx::adapter_target`, §3.4), else `bz` on PATH. The
    // version guard runs only for the default `bz`; a named target — config
    // override or host assertion — is identity the caller vouches for (one
    // trust class), so it skips the guard and the in-band MessageStart.v
    // handshake governs instead.
    let adapter_override = cfg.global.adapter.as_deref();
    let host = deps.adapter_target;
    let binary = adapter::resolve_binary(adapter_override, host);
    let expect_handshake = adapter_override.is_some() || host.is_some();
    if !expect_handshake {
        adapter::check_bz_version(deps.adapter, &binary)?;
    }

    // The workflow question has its own answer (§6 *The workflow mark*):
    // the nearest workflow mark on the agent's descent when one stands,
    // else this same governing commit ([`workflow_source`]).
    let workflow = workflow_source::resolve_workflow(workspace, &source, &commit, deps)?;
    // §4.3: every `dispatch(<role>)` binding must name a role the config
    // declares. Checked here, at the load — a workflow naming an
    // undeclared role is declined before the first model call, not at
    // the hop that finally reaches the binding. A *marked* workflow is
    // checked against the same governing `providers.yaml` the agent's
    // roles actually resolve from.
    cross::check_workflow_against_roles(&workflow, &cfg.per_repo)?;

    // §5.2: the role's context-assembly rules, read from the same config
    // commit. The role-keyed lookup is total — a role the manifest does
    // not list resolves `None` and assembles transcript-only.
    let manifest_raw = read_control(workspace, &commit, MANIFEST_FILE, deps)?;
    let manifest = Manifest::parse(&manifest_raw, &control_origin(&commit, MANIFEST_FILE))?
        .roles
        .remove(role.as_str());

    let soul_rel = format!("{SOULS_DIR}/{role}.md");
    let soul = read_control(workspace, &commit, &soul_rel, deps)?;

    Ok(WorkerConfig {
        role,
        model_id: assignment.model.clone(),
        provider_row: assignment.provider.clone(),
        tools: assignment.tools.clone(),
        config_commit: commit,
        soul,
        binary,
        workflow,
        manifest,
        expect_handshake,
    })
}

/// The agent's role (§6 role-aware resolution). A fresh root about to
/// fork has no dispatch commit yet, so it is the worker default; an
/// existing agent's role is derived from its own dispatch commit subject
/// — the single authoritative home ([`crate::prompt::role`]) — falling
/// back to the worker default for a root branch (whose subject lacks the
/// `dispatch: <role>` prefix).
fn agent_role(
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

/// Resolve the governing config commit sha for the source (§2.2) — one
/// ancestry derivation for both, asked of the fork point for a fresh
/// root and of its own ref for an existing agent
/// ([`workspace::governing_config`]).
fn config_commit(
    workspace: &Path,
    source: &ConfigSource<'_>,
    deps: &Deps<'_>,
) -> Result<String, Error> {
    let rev = match source {
        ConfigSource::Fork(fork_point) => (*fork_point).to_owned(),
        ConfigSource::Agent(agent_id) => workspace::agent_ref(agent_id),
    };
    workspace::governing_config(workspace, &rev, deps.git).map_err(|source| Error::Git {
        op: "governing config",
        source,
    })
}

/// Read one control file from the config commit's tree (§2.2).
fn read_control(
    workspace: &Path,
    commit: &str,
    path: &str,
    deps: &Deps<'_>,
) -> Result<String, Error> {
    workspace::show_control(workspace, commit, path, deps.git).map_err(|source| {
        Error::ControlRead {
            path: control_origin(commit, path),
            source,
        }
    })
}

/// A `<commit>:<path>` label for errors — the control file's one true
/// address (§2.2: control lives in the config commit, not on disk).
fn control_origin(commit: &str, path: &str) -> PathBuf {
    PathBuf::from(format!("{commit}:{path}"))
}
