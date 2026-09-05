//! The launch seam (ARCH §2.11 *A deposit into a quiescent agent starts
//! a driver*): who may launch a driver, what a launch actually does, and
//! the probe that decides whether one is warranted.
//!
//! Split out of [`super`] at bl-6a7c along the axis its tests were
//! already split on ([`super::tests::launcher`] and
//! [`super::tests::probe`]). The module above holds the inbox's paths and
//! its deposit vocabulary; this one holds the decision to run something,
//! and [`super::cli`] the verb that orchestrates the two.

use super::{baton, inbox_dir, lock};
use crate::prompt::step;
use std::io;
use std::path::{Path, PathBuf};

/// Launches a driver for a quiescent agent — the one launch seam shared
/// by the writer's post-deposit probe, the `litany scan` flush, and the
/// exit protocol's self-directed launch (§2.11). Kept as a trait so
/// every launch decision is testable with the spawn injected, and so the
/// production launch target can change without touching the callers. No
/// launcher ever decides whether launching is warranted; warrant is
/// decided by the launched driver under the lock (§2.11).
pub trait Launcher {
    /// Start a driver for `agent_id` under `workspace`. Called only
    /// with no lease held by the caller — the probe released its lease,
    /// the exiting executor released its lock — so the launched driver
    /// competes for the acquire like any other (§2.11 Writer/driver
    /// totality). Fire-and-forget: the caller never watches the driver.
    fn launch(&self, workspace: &Path, agent_id: &str) -> io::Result<()>;
}

/// The production launcher: detach-spawns `litany advance <workspace>
/// <agent>` (§6), the workflow-chain driver that takes the lease,
/// rematerializes the worktree, drains the inbox, and steps (its
/// own-branch entry is [`crate::prompt::dispatch::driver::drive`]).
///
/// The spawn is **detached per §2.11**: `setsid` in the child (its own
/// session and process group — a §2.9 stop cascade against the launching
/// process never reaches the driver, and the driver outlives a launcher
/// running inside another agent's tool subprocess or a user's script),
/// stdin and stdout bound to null (a driver reads nothing and, by the
/// §3.4 one-product convention, says nothing on stdout), **stderr
/// captured** to the agent's [`step::DRIVER_LOG_FILE`]
/// ([`driver_log`] — §2.11; the driver's declines are operator-facing
/// and a `setsid` child has no terminal to inherit), and
/// [`baton::LOCK_FD_ENV`] scrubbed (a launched driver *acquires*; only
/// an exec'd successor adopts, §6). Fire-and-forget: the child is never
/// waited on — a launcher is short-lived by design, and the unreaped
/// driver reparents to init when the launcher exits.
#[derive(Debug)]
pub struct AdvanceLauncher {
    exe: PathBuf,
}

impl AdvanceLauncher {
    /// Explicit binary path — the driver target the binding injects
    /// (`cmd::Fx::driver_target`, ARCH §2.11/§3.4) and every test picks.
    /// The library resolves no running-binary path of its own: the one
    /// `current_exe` for the launch/successor family lives at the exec
    /// binding (`src/bin/`), threaded down as this argument.
    pub fn with_exe(exe: PathBuf) -> Self {
        Self { exe }
    }
}

/// Open the append sink for a detached driver's stderr:
/// `<workspace>/steps/<agent-id>/driver.log` (§2.11,
/// [`step::DRIVER_LOG_FILE`]). The path is **derived** from the two
/// arguments every launch already carries, so capture costs no config
/// and admits no second home for the fact (`docs/PRINCIPLES.md` Single
/// source of truth): a driver's diagnostics land in the diagnostic tree
/// its step records land in (§2.3). Created with the agent's step
/// directory, since a launch may precede that agent's first step.
///
/// A failure here **declines the launch** rather than falling back to
/// null (`docs/PRINCIPLES.md` *Decline illegal operations* — silent
/// degradation is never preferable to a loud refusal): a workspace whose
/// `steps/` tree cannot be written is one the driver could not have
/// recorded a step in either, and the refusal reaches the caller's own
/// stderr, where the failure it would have swallowed is legible.
fn driver_log(workspace: &Path, agent_id: &str) -> io::Result<std::fs::File> {
    let dir = workspace.join(step::STEPS_DIR).join(agent_id);
    std::fs::create_dir_all(&dir)?;
    std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(dir.join(step::DRIVER_LOG_FILE))
}

impl Launcher for AdvanceLauncher {
    fn launch(&self, workspace: &Path, agent_id: &str) -> io::Result<()> {
        let log = driver_log(workspace, agent_id)?;
        let mut cmd = std::process::Command::new(&self.exe);
        cmd.arg("advance")
            .arg(workspace)
            .arg(agent_id)
            .env_remove(baton::LOCK_FD_ENV)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::from(log));
        // SAFETY: [`detach_into_own_session`] is async-signal-safe (a
        // single `setsid` syscall) and is the only code executed
        // between fork and exec.
        unsafe {
            use std::os::unix::process::CommandExt;
            cmd.pre_exec(detach_into_own_session);
        }
        cmd.spawn()?;
        Ok(())
    }
}

/// The driver spawn's between-fork-and-exec hook: detach the child from
/// the launching agent's process group so a §2.9 cascade against the
/// parent never reaps the driver. `setsid` failure (caller already a
/// group leader) is ignored — the spawn proceeds grouped, which only
/// widens the cascade. Called in-process by its test: counters
/// incremented in the forked child die with the `exec`.
pub(super) fn detach_into_own_session() -> std::io::Result<()> {
    // SAFETY: `setsid` takes no arguments and touches only the calling
    // process's own group and terminal membership.
    unsafe {
        libc::setsid();
    }
    Ok(())
}

/// Outcome of the post-deposit probe (§2.11 *A deposit into a quiescent
/// agent starts a driver*).
#[derive(Debug, PartialEq, Eq)]
pub enum ProbeOutcome {
    /// The branch was quiescent; a driver was launched.
    Launched,
    /// Another executor holds the lock; it will drain at its next step
    /// boundary. Nothing to launch.
    Busy,
}

/// Probe the executor lock for `agent_id` and, finding it quiescent,
/// release the probe and launch a driver (§2.11). A non-blocking
/// try-acquire whose *success* means nobody is driving: on success the
/// lease is dropped immediately — launching is not driving — before the
/// driver is launched, so the driver can win the acquire.
pub fn probe_and_launch(
    workspace: &Path,
    agent_id: &str,
    launcher: &dyn Launcher,
) -> io::Result<ProbeOutcome> {
    let dir = inbox_dir(workspace, agent_id);
    match lock::try_acquire(&dir)? {
        Some(guard) => {
            // Release the probe *before* launching so the driver's own
            // acquire is not blocked by our lease (§2.11).
            drop(guard);
            launcher.launch(workspace, agent_id)?;
            Ok(ProbeOutcome::Launched)
        }
        None => Ok(ProbeOutcome::Busy),
    }
}
