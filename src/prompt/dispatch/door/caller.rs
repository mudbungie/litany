//! **Who raised this invocation** — the one resolution both door
//! surfaces run before they do anything else
//! (`docs/DESIGN_CODE_EXECUTION.md` §2.1, §2.7).
//!
//! Everything here comes from the §3.3 stdio contract's own environment
//! plus the agent's governing config commit, never from the input: the
//! workspace and the calling agent from `LITANY_CONV_REPO` /
//! `LITANY_CONV_BRANCH` as every built-in reads them, the step to record
//! under as that agent's in-flight step
//! ([`crate::prompt::step::in_flight`], derived and not a stored
//! cursor), and the role's **effective toolset** — its `providers.yaml`
//! grant plus everything injected into its requests — read exactly where
//! the composer and the grant gate read it
//! ([`crate::prompt::dispatch::tools::injected`],
//! [`super::super::tool_step::permit`]).
//!
//! Two readers, one home. [`super::cli`] gates and executes one
//! invocation with it; the `python` built-in generates its stub module
//! from it (§2.7 — "the same resolution, read at the same moment, so
//! the module cannot offer a function the door would refuse"). A second
//! derivation of the same facts would be exactly the drift the grant
//! gate's *declaring is not permitting* rule exists to rule out.

use crate::config::ToolControl;
use crate::harness_root;
use crate::prompt::dispatch::tools::injected;
use crate::prompt::inbox::AdvanceLauncher;
use crate::prompt::resolve::{ConfigSource, resolve_worker};
use crate::prompt::step;
use crate::prompt::tool::SpawnTool;
use crate::prompt::tool::builtin::dispatch::EnvLookup;
use crate::prompt::tool::inject::{InjectedTool, ToolInjection};
use crate::prompt::tool::{ENV_CONV_BRANCH, ENV_CONV_REPO};
use crate::prompt::{Deps, NanoIdGen, RealSleeper, SpawnAdapter, SystemClock};
use std::path::{Path, PathBuf};
use thiserror::Error;

/// The calling agent, resolved. Owned throughout: the value outlives the
/// config it was read from, and both readers hold it across work of
/// their own.
pub(crate) struct Caller {
    /// Workspace root (`LITANY_CONV_REPO`, §2.2).
    pub(crate) workspace: PathBuf,
    /// The calling agent's id == its branch name (`LITANY_CONV_BRANCH`).
    pub(crate) agent: String,
    /// The agent's in-flight step directory — where a record lands.
    pub(crate) step_dir: PathBuf,
    /// The data root the executor resolves tools under (§2.2), carried
    /// so a reader that needs its own executor does not re-resolve the
    /// harness root.
    pub(crate) data_root: PathBuf,
    /// The agent's role (§4.3), as the decline text names it.
    pub(crate) role: String,
    /// The role's `providers.yaml` `tools:` grant.
    pub(crate) grant: Vec<String>,
    /// Everything injected into this role's requests (§2.7 procedure
    /// toolsets, §3.3 host-injected tools).
    pub(crate) injected: Vec<InjectedTool>,
    /// The governing workflow's `tool_control:` block, if any (§3.3).
    pub(crate) tool_control: Option<ToolControl>,
}

/// Why the caller could not be resolved at all. A *gated* invocation is
/// never one of these — a decline is the door's ordinary product.
#[derive(Debug, Error)]
pub(crate) enum Error {
    #[error("missing env var {0:?} (set by the harness per ARCH §3.3)")]
    MissingEnv(&'static str),
    #[error(
        "agent {agent:?} has no step under {workspace}: an inner invocation records \
         inside the step that raised it (ARCH §2.3)"
    )]
    NoStep { agent: String, workspace: PathBuf },
    #[error(transparent)]
    Window(#[from] crate::prompt::Error),
}

/// Resolve the calling agent from the contract environment and its
/// governing config commit.
pub(crate) fn resolve(
    env: &dyn EnvLookup,
    driver_target: &Path,
    adapter_target: Option<&Path>,
    stop: &std::sync::atomic::AtomicBool,
    injection: Option<&dyn ToolInjection>,
) -> Result<Caller, Error> {
    let workspace = PathBuf::from(require(env, ENV_CONV_REPO)?);
    let agent = require(env, ENV_CONV_BRANCH)?
        .into_string()
        .map_err(|_| Error::MissingEnv(ENV_CONV_BRANCH))?;
    let step_dir = step::in_flight(&workspace, &agent).ok_or_else(|| Error::NoStep {
        agent: agent.clone(),
        workspace: workspace.clone(),
    })?;

    let roots = harness_root::resolve().map_err(crate::prompt::Error::from)?;
    let executor =
        SpawnTool::new(&roots.data, &SystemClock, driver_target).with_injection(injection);
    let launcher = AdvanceLauncher::with_exe(driver_target.to_path_buf());
    let rng = crate::workspace::agent_name::mint::SplitMix64::from_entropy();
    let deps = Deps {
        adapter: &SpawnAdapter,
        sleeper: &RealSleeper,
        git: &crate::template::RealGit::new(),
        clock: &SystemClock,
        id_gen: &NanoIdGen,
        tool_executor: &executor,
        config_root: &roots.config,
        data_root: &roots.data,
        adapter_target,
        stop,
        launcher: &launcher,
        rng: &rng,
    };
    let worker = resolve_worker(&workspace, ConfigSource::Agent(&agent), &deps)?;
    let resolved = worker.as_resolved();
    let injected = injected(resolved.grant.role, &executor, &workspace, &agent);
    Ok(Caller {
        role: resolved.grant.role.to_string(),
        grant: resolved.grant.tools.to_vec(),
        tool_control: resolved.workflow.tool_control.clone(),
        injected,
        data_root: roots.data.clone(),
        workspace,
        agent,
        step_dir,
    })
}

/// One contract env var, or the decline naming it — the same voice the
/// built-ins decline a hand invocation with (§3.3).
fn require(env: &dyn EnvLookup, key: &'static str) -> Result<std::ffi::OsString, Error> {
    env.get(key).ok_or(Error::MissingEnv(key))
}
