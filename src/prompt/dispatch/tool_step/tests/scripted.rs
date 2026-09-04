//! The scripted executor and the step machinery every window test
//! drives against. It was the multi-tool suite's scaffolding; when that
//! suite retired with the envelope (`docs/DESIGN_CODE_EXECUTION.md` §5)
//! the fixtures stayed, because what they stub — a `ToolExecutor` that
//! answers per name and the never-reached step machinery — is the
//! window's, not the envelope's.

use super::{NoAdapter, NoLauncher, NoSleeper};
use crate::prompt::clock::SystemClock;
use crate::prompt::tool::{ExecError, ToolCall, ToolExecutor, ToolOutcome};
use crate::template::RealGit;
use serde_json::Value;
use std::cell::RefCell;
use std::sync::atomic::AtomicBool;
use tempfile::TempDir;

/// A scripted executor: records every invocation it is handed (derived
/// id, name, input) and answers per name — an `is_error` outcome for
/// `fail` names, the §2.9 group-SIGTERM signature for `kill` names, a
/// spawn fault for `fault` names, a plain `ran <name>` outcome
/// otherwise.
pub(super) struct Scripted {
    pub(super) log: RefCell<Vec<(String, String, Value)>>,
    pub(super) fail: &'static [&'static str],
    pub(super) kill: &'static [&'static str],
    pub(super) fault: &'static [&'static str],
}

impl Scripted {
    pub(super) fn new() -> Self {
        Self {
            log: RefCell::new(Vec::new()),
            fail: &[],
            kill: &[],
            fault: &[],
        }
    }
}

impl ToolExecutor for Scripted {
    fn execute(
        &self,
        invocation: ToolCall<'_>,
        _step_dir: &std::path::Path,
        _stop: &AtomicBool,
        _bound: Option<crate::config::ToolOutputBound>,
    ) -> Result<ToolOutcome, ExecError> {
        self.log.borrow_mut().push((
            invocation.id.to_string(),
            invocation.name.to_string(),
            invocation.input.clone(),
        ));
        if self.kill.contains(&invocation.name) {
            return Err(ExecError::KilledBySignal {
                name: invocation.name.to_string(),
                signal: 15,
            });
        }
        if self.fault.contains(&invocation.name) {
            return Err(ExecError::Spawn {
                name: invocation.name.to_string(),
                source: std::io::Error::other("no such binary"),
            });
        }
        Ok(ToolOutcome {
            content: format!("ran {}", invocation.name).into_bytes(),
            is_error: self.fail.contains(&invocation.name),
        })
    }
}

/// The step machinery shared by every fixture here: real git for the
/// integration path, a scratch config root, and the never-reached stubs.
pub(super) struct Fixture {
    pub(super) git: RealGit,
    clock: SystemClock,
    id_gen: crate::prompt::NanoIdGen,
    cfg: TempDir,
}

impl Fixture {
    pub(super) fn new() -> Self {
        Self {
            git: RealGit::new(),
            clock: SystemClock,
            id_gen: crate::prompt::NanoIdGen,
            cfg: TempDir::new().unwrap(),
        }
    }

    pub(super) fn deps<'a>(
        &'a self,
        exec: &'a dyn ToolExecutor,
        stop: &'a AtomicBool,
    ) -> crate::prompt::Deps<'a> {
        crate::prompt::Deps {
            adapter: &NoAdapter,
            sleeper: &NoSleeper,
            git: &self.git,
            clock: &self.clock,
            id_gen: &self.id_gen,
            tool_executor: exec,
            config_root: self.cfg.path(),
            data_root: self.cfg.path(),
            adapter_target: None,
            stop,
            launcher: &NoLauncher,
            rng: crate::workspace::agent_name::mint::test_rng(),
        }
    }
}
