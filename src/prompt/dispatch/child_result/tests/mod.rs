//! Tests for the §6 delivered-child-result and checkpoint-flush seams.
//! Real git + a real scaffolded workspace (the shapes production runs
//! against); the adapter/sleeper/tool-executor deps are never reached on
//! these paths, so they are `unreachable!` stubs.

use super::*;
use crate::config::Workflow;
use crate::prompt::adapter::AdapterRunner;
use crate::prompt::dispatch::Sleeper;
use crate::prompt::inbox::{Epitaph, Launcher, deposit_result};
use crate::prompt::tool::{ExecError, ToolCall, ToolExecutor, ToolOutcome};
use crate::prompt::{ChildDispatchRequest, NanoIdGen, SystemClock, child_dispatch};
use crate::template::{GitRunner, RealGit};
use crate::workspace::agent_worktree;
use std::cell::RefCell;
use std::ffi::OsString;
use std::io;
use std::path::Path;
use std::sync::atomic::AtomicBool;
use std::time::Duration;
use tempfile::TempDir;

struct NoAdapter;
impl AdapterRunner for NoAdapter {
    fn run(
        &self,
        _b: &OsString,
        _a: &[&str],
        _s: &[u8],
        _o: &mut dyn FnMut(&[u8]) -> io::Result<()>,
    ) -> io::Result<Vec<u8>> {
        unreachable!("adapter is never reached on the interpreter path")
    }
}
struct NoSleeper;
impl Sleeper for NoSleeper {
    fn sleep(&self, _d: Duration) {
        unreachable!("sleeper is never reached")
    }
}
struct NoTools;
impl ToolExecutor for NoTools {
    fn execute(
        &self,
        _c: ToolCall<'_>,
        _s: &Path,
        _st: &AtomicBool,
        _b: Option<crate::config::ToolOutputBound>,
    ) -> Result<ToolOutcome, ExecError> {
        unreachable!("tool executor is never reached")
    }
}
/// A [`Launcher`] recording the agent ids it was asked to start (the
/// compactor dispatch's front-door launch), swallowing the spawn.
#[derive(Default)]
struct RecLauncher {
    launched: RefCell<Vec<String>>,
}
impl Launcher for RecLauncher {
    fn launch(&self, _ws: &Path, agent: &str) -> io::Result<()> {
        self.launched.borrow_mut().push(agent.to_string());
        Ok(())
    }
}

/// Owns the deps components so [`Fx::deps`] can borrow them into one
/// [`Deps`] with the unused traits stubbed.
pub(super) struct Fx {
    git: RealGit,
    clock: SystemClock,
    id: NanoIdGen,
    adapter: NoAdapter,
    sleeper: NoSleeper,
    tools: NoTools,
    launcher: RecLauncher,
    stop: AtomicBool,
    cfg: TempDir,
}

impl Fx {
    /// The fixture's data root — the install pools the reviewer's
    /// landing judges a proposed skill name against
    /// (`docs/DESIGN_LEARNING_LOOP.md` §3). Empty unless a test seeds
    /// one, which is the ordinary shape: a workspace whose install
    /// provides no skill collides with nothing.
    pub(super) fn data_root(&self) -> &Path {
        self.cfg.path()
    }
}
impl Fx {
    pub(super) fn new() -> Self {
        Self {
            git: RealGit::new(),
            clock: SystemClock,
            id: NanoIdGen,
            adapter: NoAdapter,
            sleeper: NoSleeper,
            tools: NoTools,
            launcher: RecLauncher::default(),
            stop: AtomicBool::new(false),
            cfg: TempDir::new().unwrap(),
        }
    }
    pub(super) fn deps(&self) -> Deps<'_> {
        Deps {
            adapter: &self.adapter,
            sleeper: &self.sleeper,
            git: &self.git,
            clock: &self.clock,
            id_gen: &self.id,
            tool_executor: &self.tools,
            config_root: self.cfg.path(),
            data_root: self.cfg.path(),
            adapter_target: None,
            stop: &self.stop,
            launcher: &self.launcher,
            rng: crate::workspace::agent_name::mint::test_rng(),
        }
    }
}

/// Fork a `role` child off `parent`, add a committed work file on the
/// child branch, and deposit its result message into the dispatcher's
/// inbox — where an ordinary child's reply goes (§2.6).
/// Returns the child id. `work` is `(path, contents)` committed on the
/// child so the transfer / merge has something to move.
pub(super) fn returned_child(
    ws: &Path,
    parent: &str,
    role: &str,
    goal: &str,
    work: (&str, &str),
    fx: &Fx,
) -> String {
    returned_child_ep(ws, parent, role, goal, work, Epitaph::FinalResponse, fx)
}

/// [`returned_child`] with the deposited epitaph chosen by the test —
/// the §2.6 epitaph-gate cases (a `died` compactor return, etc.).
pub(super) fn returned_child_ep(
    ws: &Path,
    parent: &str,
    role: &str,
    goal: &str,
    work: (&str, &str),
    epitaph: Epitaph,
    fx: &Fx,
) -> String {
    let parent_wt = agent_worktree(ws, parent);
    let req = ChildDispatchRequest {
        repo: ws,
        parent_branch: parent,
        parent_worktree: &parent_wt,
        role,
        goal,
        name: None,
        fork_point: None,
        cwd: None,
        pins: crate::prompt::PinnedDocs::none(),
    };
    let child = child_dispatch::run(
        &req,
        &fx.git,
        &fx.clock,
        &fx.id,
        &fx.launcher,
        crate::workspace::agent_name::mint::test_rng(),
    )
    .unwrap();
    // Simulate the child doing its work and committing (§2.3).
    let child_wt = agent_worktree(ws, &child);
    let f = child_wt.join(work.0);
    std::fs::create_dir_all(f.parent().unwrap()).unwrap();
    std::fs::write(&f, work.1).unwrap();
    fx.git.run(&child_wt, &["add", "-A"]).unwrap();
    fx.git
        .run(&child_wt, &["commit", "-m", "child work"])
        .unwrap();
    let tip = fx
        .git
        .run_capture(&child_wt, &["rev-parse", "HEAD"])
        .unwrap();
    // A `died` deposit mirrors the §8 sweep's: the child never spoke,
    // so the result carries no body.
    let response = (epitaph != Epitaph::Died).then_some("done");
    deposit_result(
        ws,
        parent,
        &child,
        epitaph,
        tip.trim(),
        response,
        &fx.clock,
        &fx.git,
    )
    .unwrap();
    child
}

pub(super) fn workflow(yaml: &str) -> Workflow {
    Workflow::parse(yaml, Path::new("workflow.yaml")).unwrap()
}

mod cases;
mod flush_clock;
mod flush_inflight;
mod flush_reviewer;
mod gate;
mod resolve_role;
