//! The agent-invocation seam (ARCH §9.3) and failing-run bundling (§9.2).
//!
//! Real model runs are slow and expensive, so the runner never calls a
//! model directly: it invokes the agent through the [`Agent`] trait. The
//! production [`CommandAgent`] shells out to an external harness-driver
//! program (an integration is an external binary — `docs/PRINCIPLES.md`);
//! tests substitute a fake, which is what lets the whole runner be
//! exercised without live model traffic.
//!
//! The agent's own exit code is **not** the pass signal — success is
//! decided solely by the task `check` (ARCH §9.1). What the agent reports
//! back is only a [`BundleTarget`] (its workspace path and agent id), so
//! that a failing run can be archived for triage via the shipped `litany
//! bundle` (§9.2, the [`Bundler`] seam). The agent writes those two lines
//! to the file named by `LITANY_EVAL_REPORT`; absent or malformed, the
//! run is simply un-bundleable, never an error.

use std::ffi::OsString;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::process::Command;

/// One agent invocation: run the `prompt` in `workdir` (shared with the
/// task's `setup`/`check`), under an isolated `litany_home`, configured by
/// `experiment` (a `workflow.yaml`).
pub struct Dispatch<'a> {
    pub prompt: &'a str,
    pub workdir: &'a Path,
    pub litany_home: &'a Path,
    pub experiment: &'a Path,
}

/// Where a run's work landed, for archival (ARCH §9.2): the workspace and
/// the agent id `litany bundle` needs.
#[derive(Clone, Debug, PartialEq)]
pub struct BundleTarget {
    pub workspace: PathBuf,
    pub agent_id: String,
}

/// What an invocation reports back: a bundle target when the agent
/// disclosed one, else `None`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct AgentOutcome {
    pub target: Option<BundleTarget>,
}

/// The agent-invocation seam. Implementors run the agent against a
/// [`Dispatch`] and report an [`AgentOutcome`]; an `Err` means the agent
/// could not be invoked at all (a harness fault), never that the agent
/// failed the task.
pub trait Agent {
    fn dispatch(&self, d: &Dispatch) -> io::Result<AgentOutcome>;
}

/// The archival seam (ARCH §9.2): bundle a failing run's subtree.
pub trait Bundler {
    fn bundle(&self, target: &BundleTarget, dest: &Path) -> io::Result<()>;
}

/// Production [`Agent`]: invoke an external harness-driver program with
/// the prompt on argv, `workdir` as the working directory, and
/// `LITANY_HOME` / `LITANY_EXPERIMENT` / `LITANY_EVAL_REPORT` in the env.
///
/// `LITANY_EXPERIMENT` is a hand-off, not a hook: the harness reads no
/// such variable — it takes its `workflow.yaml` from the workspace's
/// config commit (ARCH §2.2) — so *applying* the experiment is the
/// driver's own work, which the shipped `litany-eval-agent` performs
/// through `litany config`. The contract is spelled out in the repo
/// README, "Run the suite".
pub struct CommandAgent {
    program: OsString,
}

impl CommandAgent {
    pub fn new(program: impl Into<OsString>) -> Self {
        Self {
            program: program.into(),
        }
    }
}

impl Agent for CommandAgent {
    fn dispatch(&self, d: &Dispatch) -> io::Result<AgentOutcome> {
        let report = d.litany_home.join("agent-report");
        // The agent's exit status is deliberately ignored: the pass
        // signal is the task `check`, never the agent's own claim (§9.1).
        Command::new(&self.program)
            .arg(d.prompt)
            .current_dir(d.workdir)
            .env("LITANY_HOME", d.litany_home)
            .env("LITANY_EXPERIMENT", d.experiment)
            .env("LITANY_EVAL_REPORT", &report)
            .status()
            // Failing to spawn is a harness fault, and the one thing the
            // operator needs to see is *which* program did not run —
            // e.g. the shipped `litany-eval-agent` before `make install`
            // has put it on PATH.
            .map_err(|e| {
                io::Error::new(
                    e.kind(),
                    format!("--agent {}: {e}", Path::new(&self.program).display()),
                )
            })?;
        Ok(AgentOutcome {
            target: read_target(&report),
        })
    }
}

/// Parse a two-line report file (`workspace` then `agent_id`) into a
/// [`BundleTarget`]. Any shortfall — missing file, missing line, empty
/// field — yields `None`.
fn read_target(path: &Path) -> Option<BundleTarget> {
    let text = fs::read_to_string(path).ok()?;
    let mut lines = text.lines();
    let workspace = lines.next()?;
    let agent_id = lines.next()?;
    if workspace.is_empty() || agent_id.is_empty() {
        return None;
    }
    Some(BundleTarget {
        workspace: PathBuf::from(workspace),
        agent_id: agent_id.to_string(),
    })
}

/// Production [`Bundler`]: `litany bundle <workspace> <agent> <dest>`.
pub struct CommandBundler {
    program: OsString,
}

impl CommandBundler {
    pub fn new(program: impl Into<OsString>) -> Self {
        Self {
            program: program.into(),
        }
    }
}

impl Bundler for CommandBundler {
    fn bundle(&self, target: &BundleTarget, dest: &Path) -> io::Result<()> {
        let status = Command::new(&self.program)
            .arg("bundle")
            .arg(&target.workspace)
            .arg(&target.agent_id)
            .arg(dest)
            .status()?;
        if status.success() {
            Ok(())
        } else {
            Err(io::Error::other(format!("litany bundle exited {status}")))
        }
    }
}
