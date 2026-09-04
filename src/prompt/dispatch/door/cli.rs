//! Production wiring for `litany invoke` — the door verb
//! (`docs/DESIGN_CODE_EXECUTION.md` §2.1, ARCH §3.4).
//!
//! One `tool_use` block on stdin, one inner invocation through
//! [`super::gate`] and the executor, the **raw** result envelope on
//! stdout, the tool's exit code out. It commits nothing: an inner
//! invocation's worktree side effects ride the enclosing invocation's
//! one tool commit (ARCH §3.3 *Commit-per-side-effect*), and its result
//! enters no transcript — the composing tool's own output is what the
//! model reads.
//!
//! The envelope is printed **unbounded**. The §3.3 bounded projection
//! bounds what enters the *transcript*; an inner result enters a
//! program, which is exactly the consumer that can filter it, and the
//! full bytes are on disk in the record either way.
//!
//! Everything the verb needs about *whose* invocation this is is
//! [`super::caller`]'s — the workspace, the calling agent, the step to
//! record under and the effective toolset the gates adjudicate against,
//! all read from the §3.3 contract environment and the governing config
//! commit rather than from the input. The `python` built-in resolves the
//! same value for the same agent, which is what keeps a program's stub
//! module and this verb's grant gate from drifting.

use super::caller::{Caller, resolve};
use super::{Passage, Verdict, gate};
use crate::prompt::SystemClock;
use crate::prompt::tool::builtin::dispatch::EnvLookup;
use crate::prompt::tool::inject::ToolInjection;
use crate::prompt::tool::{SpawnTool, ToolCall, ToolExecutor, envelope};
use serde::Deserialize;
use serde_json::Value;
use std::path::Path;
use thiserror::Error;

/// The exit code a settled-without-running invocation carries: no child
/// ran, so there is no exit code to convey and none is invented (§3.3 —
/// the same stance [`crate::prompt::dispatch::tool_step::seam`] takes
/// when it declines to render an envelope for a refusal). The reason is
/// on stdout; the code only says *not zero*.
const NO_CHILD: i32 = 1;

/// One `tool_use` block, exactly as a tool control reads it on stdin
/// (ARCH §3.3 *Tool control*). An omitted `input` is the empty object —
/// the general path with the field absent, matching a tool whose schema
/// requires nothing.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Block {
    id: String,
    name: String,
    #[serde(default = "empty_input")]
    input: Value,
}

fn empty_input() -> Value {
    Value::Object(serde_json::Map::new())
}

/// Why the door could not adjudicate an invocation at all. A *gated*
/// invocation is never one of these — a decline is the verb's ordinary
/// product, printed and exited non-zero, exactly as the model reads it.
#[derive(Debug, Error)]
pub(crate) enum Error {
    #[error("read the tool_use block from stdin: {0}")]
    Stdin(std::io::Error),
    #[error(
        "malformed tool_use block: {0}. Expected \
         {{\"id\": \"<id>\", \"name\": \"<tool>\", \"input\": {{...}}}}"
    )]
    Malformed(serde_json::Error),
    #[error(transparent)]
    Caller(#[from] super::caller::Error),
    /// The gates' own failure — a tool control that could not answer
    /// (§3.3 *Tool control*: a control fault is never a pass).
    #[error(transparent)]
    Window(#[from] crate::prompt::Error),
    #[error(transparent)]
    Exec(#[from] crate::prompt::tool::ExecError),
    #[error("write the result envelope: {0}")]
    Stdout(std::io::Error),
}

/// Run one invocation through the door. Returns the process exit code
/// the verb is to end with; the bytes the caller prints have already
/// been written to `stdout`.
#[allow(clippy::too_many_arguments)] // the binding's injections, one door
pub(crate) fn run(
    env: &dyn EnvLookup,
    stdin: &mut dyn std::io::Read,
    stdout: &mut dyn std::io::Write,
    driver_target: &Path,
    adapter_target: Option<&Path>,
    stop: &std::sync::atomic::AtomicBool,
    injection: Option<&dyn ToolInjection>,
) -> Result<i32, Error> {
    let mut buf = Vec::new();
    stdin.read_to_end(&mut buf).map_err(Error::Stdin)?;
    let block: Block = serde_json::from_slice(&buf).map_err(Error::Malformed)?;
    let caller: Caller = resolve(env, driver_target, adapter_target, stop, injection)?;
    let executor =
        SpawnTool::new(&caller.data_root, &SystemClock, driver_target).with_injection(injection);
    let passage = Passage {
        id: &block.id,
        name: &block.name,
        input: &block.input,
        role: &caller.role,
        grant: &caller.grant,
        injected: &caller.injected,
        tool_control: caller.tool_control.as_ref(),
        conv_repo: &caller.workspace,
        conv_id: &caller.agent,
        stop,
    };
    let (bytes, code) = match gate(&passage)? {
        None => (STOPPED.as_bytes().to_vec(), NO_CHILD),
        Some(Verdict::Declined(text)) => (text.into_bytes(), NO_CHILD),
        Some(Verdict::Proceed) => {
            let call = ToolCall {
                id: &block.id,
                name: &block.name,
                input: &block.input,
            };
            // `None` bound: the raw envelope, per this module's header.
            let outcome = executor.execute(call, &caller.step_dir, stop, None)?;
            let code = envelope::stated_exit_code(&outcome.content).unwrap_or(NO_CHILD);
            (outcome.content, code)
        }
    };
    stdout.write_all(&bytes).map_err(Error::Stdout)?;
    Ok(code)
}

/// The in-band text for the §2.9 stop landing on the control mid-consult
/// — the tool window ceases there, and so does this invocation.
const STOPPED: &str = "the harness is stopping (ARCH §2.9): this invocation did not run.";
