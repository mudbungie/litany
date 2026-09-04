//! Production wiring for `litany advance <workspace> <agent>` (§6).
//!
//! Mirrors the `litany prompt` deps wiring — the same real components,
//! the same discipline of keeping the bin a thin shim: [`cli_run`] does
//! everything up to the `exec` itself, returning the fully prepared
//! successor [`Command`] (args, `LITANY_LOCK_FD`, close-on-exec cleared
//! — [`baton::successor_command`]) for the bin to `exec`. The exec
//! stays in the bin because a successful `execve` never returns — the
//! library boundary is the last observable point of this process.

use super::{AdvanceOutcome, run};
use crate::harness_root;
use crate::prompt::inbox::{self, AdvanceLauncher, baton};
use crate::prompt::resolve::{ConfigSource, resolve_worker};
use crate::prompt::tool::inject::ToolInjection;
use crate::prompt::{Deps, Error, RealSleeper, SpawnAdapter, SystemClock};
use crate::prompt::{NanoIdGen, tool::SpawnTool};
use std::ffi::OsStr;
use std::path::Path;
use std::process::Command;
use std::sync::atomic::AtomicBool;

/// What the bin does after one hop: nothing, or exec the successor.
#[derive(Debug)]
pub enum AdvanceHandoff {
    /// The hop completed in this process — a no-op, an already-driven
    /// exit, or a terminal event whose exit protocol already ran. The
    /// hop's outcome carries no product here: `cmd::advance::outcome_of`
    /// maps every completed hop to a product-less `Outcome::Quiet`
    /// (§3.4), so nothing downstream reads it.
    Done,
    /// The step emitted `tool_use`: exec this prepared successor
    /// command (§6 exec baton). Only `exec` remains — the lease fd is
    /// already inheritable and published in the command's environment.
    Exec(Command),
}

/// Run one production hop: guard the target's existence
/// ([`crate::workspace::require_agent`], §2.3 — before any lease, so a
/// refusal writes nothing), take the lease (adopting a predecessor's
/// [`baton::LOCK_FD_ENV`] fd from the live environment, else
/// acquiring), drive [`run`] with the real components, and prepare the
/// §6 handoff. `driver_target` is the running-binary path the exec
/// binding injects (`cmd::Fx::driver_target`, §3.4) — it is the
/// successor `execve` target, the launcher's detached-spawn target,
/// *and* the §3.3 tool resolver's third hop, so the library resolves no
/// `current_exe` of its own; `stop` is the
/// executor's injected SIGTERM flag (`cmd::Fx::stop`, §2.9).
pub fn cli_run(
    workspace: &Path,
    agent_id: &str,
    driver_target: &Path,
    adapter_target: Option<&Path>,
    stop: &AtomicBool,
    injection: Option<&dyn ToolInjection>,
) -> Result<AdvanceHandoff, Error> {
    cli_run_with(
        workspace,
        agent_id,
        std::env::var_os(baton::LOCK_FD_ENV).as_deref(),
        driver_target,
        adapter_target,
        stop,
        injection,
    )
}

/// [`cli_run`] with the lease env value injected — the same
/// env-as-parameter discipline as `inbox::resolve_cli_sender`, so the
/// adopt arm is exercisable without mutating the test process's
/// environment.
fn cli_run_with(
    workspace: &Path,
    agent_id: &str,
    lease_env: Option<&OsStr>,
    driver_target: &Path,
    adapter_target: Option<&Path>,
    stop: &AtomicBool,
    injection: Option<&dyn ToolInjection>,
) -> Result<AdvanceHandoff, Error> {
    crate::workspace::require(workspace)?;
    // §2.3 existence guard, ahead of the lease: the `agents/*` refs are
    // the registry, so a hop at a name with no ref is refused here —
    // before `take_lease` would `mkdir` the inbox and manufacture the
    // very orphan directory `litany scan` reports as debris. Not folded
    // into the §2.11 lost-lease no-op (`Ok(Done)`): that is a live agent
    // already driven, this is an operator typo, and it exits 1.
    crate::workspace::require_agent(
        workspace,
        agent_id,
        "a hop drives an existing agent (ARCH §2.3: the `agents/*` refs are the registry)",
        &crate::template::RealGit::new(),
    )?;
    let inbox_dir = inbox::inbox_dir(workspace, agent_id);
    let lease = match baton::take_lease(lease_env, &inbox_dir) {
        Ok(Some(lease)) => lease,
        Ok(None) => return Ok(AdvanceHandoff::Done),
        Err(baton::LeaseError::Acquire(source)) => {
            return Err(Error::ExecutorLock {
                path: inbox_dir,
                source,
            });
        }
        Err(baton::LeaseError::Adopt(e)) => {
            return Err(Error::LeaseAdopt {
                agent: agent_id.to_string(),
                detail: e.to_string(),
            });
        }
    };

    let roots = harness_root::resolve()?;
    let tool_executor =
        SpawnTool::new(&roots.data, &SystemClock, driver_target).with_injection(injection);
    let launcher = AdvanceLauncher::with_exe(driver_target.to_path_buf());
    let deps = Deps {
        adapter: &SpawnAdapter,
        sleeper: &RealSleeper,
        git: &crate::template::RealGit::new(),
        clock: &SystemClock,
        id_gen: &NanoIdGen,
        tool_executor: &tool_executor,
        config_root: &roots.config,
        data_root: &roots.data,
        adapter_target,
        stop,
        launcher: &launcher,
        rng: &crate::workspace::agent_name::mint::SplitMix64::from_entropy(),
    };

    let outcome = run(workspace, agent_id, Some(lease), &deps, &mut || {
        resolve_worker(workspace, ConfigSource::Agent(agent_id), &deps)
    })?;
    handoff(driver_target, workspace, agent_id, outcome)
}

/// Map a hop's outcome to the bin's next act (§6 step 5): tools ran →
/// the prepared successor exec; anything else completed here.
fn handoff(
    exe: &Path,
    workspace: &Path,
    agent_id: &str,
    outcome: AdvanceOutcome,
) -> Result<AdvanceHandoff, Error> {
    match outcome {
        AdvanceOutcome::ToolsPending(lease) => Ok(AdvanceHandoff::Exec(baton::successor_command(
            exe, workspace, agent_id, lease,
        )?)),
        _ => Ok(AdvanceHandoff::Done),
    }
}

#[cfg(test)]
mod tests;
