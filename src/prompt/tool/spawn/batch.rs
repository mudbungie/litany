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
//! router ([`SpawnTool::route`]) or a subprocess
//! ([`SpawnTool::spawn_one`]).
//!
//! The three phases were split when the executor also answered a *fan*
//! of calls at once — the `parallel` multi-tool envelope, retired with
//! the multi-tool (`docs/DESIGN_CODE_EXECUTION.md` §5): a program fans
//! with a thread pool over its own stub module now, so the harness owns
//! no batch API. The split stays because it is what keeps the clock, the
//! git runner and the PATH lookup out of anything that blocks — the
//! reason a host router, whose `Sync`-ness litany holds nothing about,
//! was always answered on the calling thread.

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
        let extra_env = caller.env(call.id);
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
            cwd: &prepared.caller.cwd,
            stop,
        })
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
