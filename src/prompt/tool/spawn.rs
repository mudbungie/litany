//! Production [`super::ToolExecutor`] — resolves the tool binary,
//! delegates spawn / capture / cascade to [`super::subprocess`], and
//! lands the per-tool-call disk record under `<step_dir>/tools/<tool-id>/`.
//!
//! Resolution order, per ARCH §3.3:
//!
//! 1. `<data_root>/tools/litany-tool-<name>` (installed by `make
//!    install`).
//! 2. `litany-tool-<name>` on `PATH` (mirroring §4.4 adapter discovery).
//! 3. In-process fallback: `<driver target> tool <name>` — re-entry
//!    into the same dispatcher, matching PRINCIPLES "Everyone uses the
//!    front door". The target is the one the binding injected
//!    (`cmd::Fx::driver_target`), never a name this module resolves:
//!    ARCH §2.11, "the driver target is injected at the binding, not
//!    resolved by name". Under the exec binding that is the `litany`
//!    image; under a linked host it is the host's own re-exec target or
//!    a PATH-resolved `litany` — never the host binary itself, which
//!    carries no `tool` verb of its own.
//!
//! Ahead of all three sits the binding's optional **host injection**
//! ([`super::inject`], §3.3 *Host-injected tools*): the router is
//! consulted first, and an invocation it owns never reaches resolution at
//! all. Everything around the answer is unchanged — the same
//! `input.json` / `output.json` record, the same bounded projection, the
//! same result envelope — so a routed tool and a spawned one are one
//! contract with two backends.

mod batch;
mod caller;
pub(super) mod lookup;

use batch::Prepared;

pub use lookup::{EnvPath, PathLookup};

use super::inject::{InjectedTool, RoutedCapture, ToolInjection};
use super::subprocess::{Captured, SpawnArgs, spawn_and_capture};
use super::{
    ExecError, IN_PROCESS_SUBCOMMAND, INPUT_FILE, OUTPUT_FILE, ToolCall, ToolExecutor,
    ToolInputRecord, ToolOutcome, ToolOutputRecord, atomic_write_json, bound, envelope,
    tool_call_dir,
};
use crate::config::ToolOutputBound;
use crate::prompt::Clock;
use crate::template::{GitRunner, RealGit};
use std::ffi::OsString;
use std::os::unix::process::ExitStatusExt;
use std::path::Path;
use std::sync::atomic::AtomicBool;
use std::time::Duration;

/// Production [`ToolExecutor`]. Constructed at the executor's entry
/// point (§2.1) rather than held long-term, so the borrow of
/// `data_root` and `clock` stays scoped to one step loop.
pub struct SpawnTool<'a> {
    data_root: &'a Path,
    clock: &'a dyn Clock,
    driver_target: &'a Path,
    deadline: Duration,
    etxtbsy_budget: u32,
    path_lookup: Box<dyn PathLookup + 'a>,
    git: Box<dyn GitRunner + 'a>,
    injection: Option<&'a dyn ToolInjection>,
}

impl<'a> SpawnTool<'a> {
    /// Build a [`SpawnTool`] over the live `PATH` and the default §3.3
    /// deadline. `driver_target` is the binding-injected re-entry path
    /// (`cmd::Fx::driver_target`) the third hop addresses as
    /// `<driver_target> tool <name>`.
    pub fn new(data_root: &'a Path, clock: &'a dyn Clock, driver_target: &'a Path) -> Self {
        Self {
            data_root,
            clock,
            driver_target,
            deadline: super::DEFAULT_TOOL_DEADLINE,
            etxtbsy_budget: super::subprocess::ETXTBSY_RETRY_ATTEMPTS,
            path_lookup: Box::new(EnvPath),
            git: Box::new(RealGit::new()),
            injection: None,
        }
    }

    /// Install the binding's tool injection (`cmd::Fx::tool_injection`,
    /// ARCH §3.3 *Host-injected tools*) — the definitions this executor
    /// declares beyond the pool and the router consulted ahead of
    /// [`Self::resolve`]. `None` is the exec binding's default and leaves
    /// every path exactly as it was.
    pub fn with_injection(mut self, injection: Option<&'a dyn ToolInjection>) -> Self {
        self.injection = injection;
        self
    }

    /// Override how many spawn attempts ride out `ETXTBSY` — an attempt
    /// count, never a wall-clock deadline (README's determinism rule,
    /// bl-edf6). A test that means to exercise the retry arm sets a
    /// count its fixture's hold cannot outlast, and one that means to
    /// exercise the give-up arm sets a small count against a permanent
    /// hold — both arms are then structural, with no clock in the
    /// verdict at all (bl-7a3f).
    #[cfg(test)] // test-only builder
    pub fn with_etxtbsy_budget(mut self, attempts: u32) -> Self {
        self.etxtbsy_budget = attempts;
        self
    }

    /// Override the SIGTERM-to-SIGKILL grace. Tests use a sub-second
    /// deadline so the cascade is observable without a 5s wait.
    #[cfg(test)] // test-only builder
    pub fn with_deadline(mut self, d: Duration) -> Self {
        self.deadline = d;
        self
    }

    /// Override the PATH lookup — used by tests to drive the second hop
    /// without mutating the live `PATH`.
    #[cfg(test)] // test-only builder
    pub fn with_path_lookup(mut self, l: Box<dyn PathLookup + 'a>) -> Self {
        self.path_lookup = l;
        self
    }

    /// Override the git runner the working-directory mark is read through
    /// (§3.3) — tests drive the moved-cwd arms without founding a repo.
    #[cfg(test)] // test-only builder
    pub fn with_git(mut self, g: Box<dyn GitRunner + 'a>) -> Self {
        self.git = g;
        self
    }

    /// Apply the §3.3 resolution order. Returns `(binary, args)` so
    /// the caller can spawn it without re-deciding the in-process
    /// case. Total: the third hop is the injected driver target, so
    /// there is no unresolvable case — a name no binary answers to is
    /// declined by the dispatcher behind the front door
    /// (`builtin::Error::Unknown`), not by this lookup.
    fn resolve(&self, name: &str) -> (OsString, Vec<OsString>) {
        let external_name = format!("{}{}", super::EXTERNAL_PREFIX, name);
        let harness_path = self.data_root.join(super::TOOLS_DIR).join(&external_name);
        if harness_path.is_file() {
            return (harness_path.into_os_string(), Vec::new());
        }
        if let Some(p) = self.path_lookup.which_on_path(&external_name) {
            return (p.into_os_string(), Vec::new());
        }
        let args = vec![OsString::from(IN_PROCESS_SUBCOMMAND), OsString::from(name)];
        (self.driver_target.as_os_str().to_owned(), args)
    }
}

impl<'a> ToolExecutor for SpawnTool<'a> {
    fn execute(
        &self,
        call: ToolCall<'_>,
        step_dir: &Path,
        stop: &AtomicBool,
        output_bound: Option<ToolOutputBound>,
    ) -> Result<ToolOutcome, ExecError> {
        let prepared = self.prepare(call, step_dir)?;
        let started_at = self.clock.now_iso8601();
        // The host router, ahead of resolution (§3.3): an invocation it
        // owns is answered here and never spawns.
        if let Some(routed) = self.route(&prepared, call, stop) {
            let ended_at = self.clock.now_iso8601();
            return self.land(&prepared, &routed, output_bound, &started_at, &ended_at);
        }
        let captured =
            spawn_and_capture(&prepared.spawn_args(stop, self.deadline, self.etxtbsy_budget))?;
        let ended_at = self.clock.now_iso8601();
        self.finish(&prepared, captured, output_bound, &started_at, &ended_at)
    }

    /// The definitions the binding injected, if any (ARCH §3.3
    /// *Host-injected tools*). The composer and the grant gate both read
    /// this, so a host declares and permits with one statement.
    fn injected(&self) -> Vec<InjectedTool> {
        self.injection.map(ToolInjection::tools).unwrap_or_default()
    }

    /// The overlapping implementation a `parallel` multi-tool envelope
    /// reaches (ARCH §3.3). Every call is prepared on this thread, the
    /// blocking waits run together in one [`std::thread::scope`], and
    /// every record is landed back on this thread — so the clock, the
    /// git runner and the PATH lookup never cross a thread boundary
    /// ([`batch`] says why).
    ///
    /// The window is one pair of clock reads for the whole fan, not
    /// one per tool call: under `parallel` the calls genuinely do start
    /// together, and `self.clock` is not shared into the scope to say
    /// otherwise.
    fn execute_all(
        &self,
        calls: &[ToolCall<'_>],
        step_dir: &Path,
        stop: &AtomicBool,
        output_bound: Option<ToolOutputBound>,
    ) -> Vec<Result<ToolOutcome, ExecError>> {
        let prepared: Vec<Result<Prepared, ExecError>> = calls
            .iter()
            .map(|call| self.prepare(*call, step_dir))
            .collect();
        let started_at = self.clock.now_iso8601();
        // Routed invocations are answered here, in list order, before the
        // scope opens: the router runs in this thread (it is the host's
        // code, bound by no `Sync`), and what it owns never spawns. The
        // fan's concurrency is unaffected for everything it declines.
        let routed: Vec<Option<RoutedCapture>> = prepared
            .iter()
            .zip(calls)
            .map(|(p, call)| p.as_ref().ok().and_then(|p| self.route(p, *call, stop)))
            .collect();
        // Copied out so the scope's closures capture two scalars
        // instead of `self` — `self` holds the clock, the git runner
        // and the PATH lookup, none of which are `Sync` and none of
        // which the blocking phase needs.
        let (deadline, etxtbsy_budget) = (self.deadline, self.etxtbsy_budget);
        let captured: Vec<Result<Captured, ExecError>> = std::thread::scope(|scope| {
            let handles: Vec<_> = prepared
                .iter()
                .zip(&routed)
                .filter_map(|(p, routed)| p.as_ref().ok().filter(|_| routed.is_none()))
                .map(|p| {
                    scope.spawn(move || {
                        spawn_and_capture(&p.spawn_args(stop, deadline, etxtbsy_budget))
                    })
                })
                .collect();
            handles
                .into_iter()
                // A panicking capture is a harness fault, not a tool
                // failure: re-raise it here so it reads exactly as it
                // would have from `execute`.
                .map(|h| h.join().unwrap_or_else(|p| std::panic::resume_unwind(p)))
                .collect()
        });
        let ended_at = self.clock.now_iso8601();
        // The routing verdicts ride *with* their calls rather than in a
        // second iterator, so a failed prepare cannot slip them out of
        // step; only the captures, which exist for spawned calls alone,
        // are stepped by hand.
        let mut captured = captured.into_iter();
        prepared
            .into_iter()
            .zip(routed)
            .map(|(prepared, routed)| {
                let prepared = prepared?;
                match routed {
                    Some(routed) => {
                        self.land(&prepared, &routed, output_bound, &started_at, &ended_at)
                    }
                    None => self.finish(
                        &prepared,
                        captured.next().expect("one capture per spawned call")?,
                        output_bound,
                        &started_at,
                        &ended_at,
                    ),
                }
            })
            .collect()
    }
}

/// Build the §3.3 / §2.10 "killed by a signal that was not the
/// harness's SIGTERM" fault. Extracts the signal number from the
/// kernel-reported [`ExitStatus`] (defaulting to 0 if some platform
/// reports neither code nor signal — should not happen on Linux).
pub(super) fn killed_by_signal(name: &str, status: &std::process::ExitStatus) -> ExecError {
    let signal = status.signal().unwrap_or(0);
    ExecError::KilledBySignal {
        name: name.to_string(),
        signal,
    }
}
