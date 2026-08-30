//! [`SpawnTool`]'s three phases, and its two interchangeable middles.
//!
//! One tool call is **prepare** (resolve the caller's worktree, land the
//! input record, resolve the binary — all of it reaching `self.git`,
//! `self.path_lookup`, `self.clock`), then **answer**, then **land**
//! (bound the streams, render the envelope, write the output record —
//! `self` again, via the caller's record path).
//!
//! The middle is whichever backend the binding installed (`super`'s
//! module docs), and both produce the same [`RoutedCapture`]: a host
//! router ([`SpawnTool::route`], [`SpawnTool::route_fan`]) or a
//! subprocess ([`SpawnTool::spawn_one`], [`SpawnTool::spawn_fan`]). Only
//! the spawning one can overlap, and the split is what lets it: under
//! [`SpawnTool::execute_all`] (ARCH §3.3 *The multi-tool*, `execution:
//! "parallel"`) nothing but the blocking wait crosses into the scope,
//! carrying owned bytes, `&Path` and the `&AtomicBool` stop flag. The
//! clock, the git runner and the PATH lookup stay on the calling thread
//! and need no `Sync` bound (PRINCIPLES, severability) — which is also
//! why a host router, whose `Sync`-ness litany holds nothing about, runs
//! in list order on that same thread.

use super::caller::Caller;
use super::{
    Captured, ExecError, INPUT_FILE, OUTPUT_FILE, SpawnArgs, SpawnTool, ToolCall, ToolInputRecord,
    ToolOutcome, ToolOutputRecord, atomic_write_json, bound, envelope, killed_by_signal,
    spawn_and_capture, tool_call_dir,
};
use crate::config::ToolOutputBound;
use crate::prompt::tool::inject::{RoutedCall, RoutedCapture, ToolInjection};
use std::ffi::OsString;
use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::time::Duration;

/// One call's answer, paired with the [`Prepared`] that produced it so
/// the landing phase cannot step them out of alignment — or the
/// preparation / spawn failure that stands in its place.
pub(super) type Answered = Result<(Prepared, RoutedCapture), ExecError>;

/// Everything one call needs to spawn, resolved and owned so the
/// blocking phase borrows nothing from the executor.
pub(super) struct Prepared {
    dir: PathBuf,
    caller: Caller,
    binary: OsString,
    args: Vec<OsString>,
    stdin: Vec<u8>,
    extra_env: Vec<(&'static str, OsString)>,
    name: String,
}

impl Prepared {
    /// Borrow this call's owned parts into the request
    /// [`spawn_and_capture`] takes. `stop` is the only thing shared
    /// with the rest of the harness, and `&AtomicBool` is `Sync`.
    pub(super) fn spawn_args<'x>(
        &'x self,
        stop: &'x AtomicBool,
        deadline: Duration,
        etxtbsy_budget: u32,
    ) -> SpawnArgs<'x> {
        SpawnArgs {
            binary: &self.binary,
            args: &self.args,
            stdin_bytes: &self.stdin,
            extra_env: &self.extra_env,
            cwd: &self.caller.cwd,
            stop,
            deadline,
            etxtbsy_budget,
            tool_name: &self.name,
        }
    }
}

impl<'a> SpawnTool<'a> {
    /// Phase 1: resolve the calling agent's worktree, create the
    /// per-tool-call record directory, land `input.json`, and resolve the
    /// binary. Fails before any process is started.
    pub(super) fn prepare(
        &self,
        call: ToolCall<'_>,
        step_dir: &std::path::Path,
    ) -> Result<Prepared, ExecError> {
        let caller =
            Caller::resolve(step_dir, &*self.git).ok_or_else(|| ExecError::NoWorktree {
                name: call.name.to_string(),
                step_dir: step_dir.to_path_buf(),
            })?;
        let dir = tool_call_dir(step_dir, call.id);
        std::fs::create_dir_all(&dir).map_err(|source| ExecError::Io {
            dir: dir.clone(),
            source,
        })?;
        let input_record = ToolInputRecord {
            id: call.id.to_string(),
            name: call.name.to_string(),
            input: call.input.clone(),
        };
        atomic_write_json(&dir, INPUT_FILE, &input_record)?;
        let (binary, args) = self.resolve(call.name);
        let extra_env = caller.env();
        Ok(Prepared {
            dir,
            caller,
            binary,
            args,
            stdin: serde_json::to_vec(call.input).expect("Value is always serializable"),
            extra_env,
            name: call.name.to_string(),
        })
    }

    /// The spawning backend, one call: block on the subprocess, then
    /// classify the exit — a signal that was not the harness's SIGTERM is
    /// a §2.10 harness fault, not a tool failure.
    pub(super) fn spawn_one(
        &self,
        prepared: &Prepared,
        stop: &AtomicBool,
    ) -> Result<RoutedCapture, ExecError> {
        let captured =
            spawn_and_capture(&prepared.spawn_args(stop, self.deadline, self.etxtbsy_budget))?;
        classify(prepared, captured)
    }

    /// The spawning backend, a whole fan: the blocking waits overlap in
    /// one [`std::thread::scope`], which the two scalars copied out below
    /// are what makes possible — the closures capture those instead of
    /// `self`, which holds the clock, the git runner and the PATH lookup,
    /// none of them `Sync` and none of them needed to block.
    pub(super) fn spawn_fan(
        &self,
        prepared: Vec<Result<Prepared, ExecError>>,
        stop: &AtomicBool,
    ) -> Vec<Answered> {
        let (deadline, etxtbsy_budget) = (self.deadline, self.etxtbsy_budget);
        let captured: Vec<Result<Captured, ExecError>> = std::thread::scope(|scope| {
            let handles: Vec<_> = prepared
                .iter()
                .filter_map(|p| p.as_ref().ok())
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
        // Captures exist only for the calls that got as far as a spawn,
        // so they are stepped by hand against the full prepared list.
        let mut captured = captured.into_iter();
        prepared
            .into_iter()
            .map(|prepared| {
                let prepared = prepared?;
                let captured = captured.next().expect("one capture per spawned call")?;
                let captured = classify(&prepared, captured)?;
                Ok((prepared, captured))
            })
            .collect()
    }

    /// The routing backend, one call. The caller identity handed over is
    /// the same one a subprocess reads from its environment, derived once
    /// in [`Self::prepare`] so a routed call and a spawned one cannot
    /// disagree about whose call it is. The host answers every name it is
    /// given ([`ToolInjection::route`] is total), so there is no verdict
    /// here and nothing to fall through to.
    pub(super) fn route(
        &self,
        injection: &dyn ToolInjection,
        prepared: &Prepared,
        call: ToolCall<'_>,
        stop: &AtomicBool,
    ) -> RoutedCapture {
        injection.route(RoutedCall {
            id: call.id,
            name: call.name,
            input: call.input,
            workspace: &prepared.caller.workspace,
            agent: &prepared.caller.agent_id,
            stop,
        })
    }

    /// The routing backend, a whole fan: answered in list order on this
    /// thread. A call whose preparation failed is never routed — the
    /// failure is its result, exactly as under the spawning backend.
    pub(super) fn route_fan(
        &self,
        prepared: Vec<Result<Prepared, ExecError>>,
        calls: &[ToolCall<'_>],
        injection: &dyn ToolInjection,
        stop: &AtomicBool,
    ) -> Vec<Answered> {
        prepared
            .into_iter()
            .zip(calls)
            .map(|(prepared, call)| {
                let prepared = prepared?;
                let captured = self.route(injection, &prepared, *call, stop);
                Ok((prepared, captured))
            })
            .collect()
    }

    /// Phase 3, over the three facts a finished tool call has — exit
    /// code, stdout, stderr — whether a subprocess or a host router
    /// produced them: bound the streams (§3.3 *Bounded transcript
    /// projection*, before the envelope is rendered around them, since
    /// the envelope's header is structure and never cappable content),
    /// render the result envelope, and land `output.json` with the full
    /// bytes. **One landing for both backends**, and it belongs to the
    /// executor rather than to whatever answered: it is what makes a
    /// routed tool indistinguishable from a spawned one downstream, and
    /// what a host cannot forget to do (`docs/DESIGN_TOOL_INJECTION.md`
    /// §3.2).
    pub(super) fn land(
        &self,
        prepared: &Prepared,
        captured: &RoutedCapture,
        output_bound: Option<ToolOutputBound>,
        started_at: &str,
        ended_at: &str,
    ) -> Result<ToolOutcome, ExecError> {
        let exit_code = captured.exit_code;
        let record = prepared.caller.record_rel(&prepared.dir).join(OUTPUT_FILE);
        let stdout = bound::apply(&captured.stdout, "stdout", output_bound, &record);
        let stderr = bound::apply(&captured.stderr, "stderr", output_bound, &record);
        let content = envelope::render(exit_code, &stdout, &stderr);
        let output_record = ToolOutputRecord {
            stdout: String::from_utf8_lossy(&captured.stdout).into_owned(),
            stderr: String::from_utf8_lossy(&captured.stderr).into_owned(),
            exit_code,
            started_at: started_at.to_string(),
            ended_at: ended_at.to_string(),
        };
        atomic_write_json(&prepared.dir, OUTPUT_FILE, &output_record)?;
        Ok(ToolOutcome {
            content,
            is_error: exit_code != 0,
        })
    }
}

/// Read a finished subprocess as the same three facts a router answers
/// in. A signal that was not the harness's SIGTERM has no exit code and
/// is a §2.10 harness fault rather than a tool failure, so it declines
/// here instead of becoming a capture.
fn classify(prepared: &Prepared, captured: Captured) -> Result<RoutedCapture, ExecError> {
    match captured.status.code() {
        Some(exit_code) => Ok(RoutedCapture {
            stdout: captured.stdout,
            stderr: captured.stderr,
            exit_code,
        }),
        None => Err(killed_by_signal(&prepared.name, &captured.status)),
    }
}
