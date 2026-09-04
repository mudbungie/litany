//! The **tool-control seam** (ARCH §3.3 *Tool control*): the one point
//! where a configured control sits in front of a tool invocation.
//!
//! [`adjudicate`] is total over the config: with no `tool_control:`
//! block it answers [`Gate::Proceed`] without spawning anything — the
//! general path with the policy absent, zero behavior change — and with
//! one it maps the control's wire verdict ([`control::consult`]) onto
//! what the tool window does next. Control faults **fail closed**
//! ([`crate::prompt::Error::ToolControl`]); the one exception is a
//! control felled by the harness's own stop SIGTERM, which is the stop
//! (§2.9 step 3), classified by the flag exactly as for tools.

use crate::config::ToolControl;
use crate::prompt::Error;
use crate::prompt::tool::control::{self, ControlError, ControlRequest, Verdict};
use serde_json::Value;
use std::path::Path;
use std::sync::atomic::AtomicBool;

/// What the seam decided for one invocation.
pub(in crate::prompt::dispatch) enum Gate {
    /// Enter the executor (no control configured, or it passed).
    Proceed,
    /// Never enter the executor; commit `reason` as an in-band
    /// `is_error` `tool_result` (the grant-decline idiom).
    Refuse(String),
    /// Park the invocation before execution: write the hold mark and
    /// cease the window (§3.3 — released by re-adjudication on the next
    /// drive).
    Hold(String),
    /// The stop cascade felled the control mid-consult: cease the
    /// window for the stopped exit, same as a tool cut down by the
    /// group SIGTERM.
    Stopped,
}

/// Consult the configured control (if any) about one invocation.
#[allow(clippy::too_many_arguments)] // one seam, every fact it adjudicates on
pub(in crate::prompt::dispatch) fn adjudicate(
    configured: Option<&ToolControl>,
    role: &str,
    id: &str,
    name: &str,
    input: &Value,
    conv_repo: &Path,
    agent_id: &str,
    stop: &AtomicBool,
) -> Result<Gate, Error> {
    let Some(tool_control) = configured else {
        return Ok(Gate::Proceed);
    };
    let request = ControlRequest {
        id,
        name,
        input,
        role,
        agent_id,
    };
    match control::consult(&tool_control.command, &request, conv_repo, stop) {
        Ok(Verdict::Pass) => Ok(Gate::Proceed),
        Ok(Verdict::Refuse { reason }) => Ok(Gate::Refuse(reason)),
        Ok(Verdict::Hold { reason }) => Ok(Gate::Hold(reason)),
        Err(ControlError::KilledBySignal { .. }) if super::stop_signal::stopped(stop) => {
            Ok(Gate::Stopped)
        }
        Err(source) => Err(Error::ToolControl {
            command: tool_control.command.clone(),
            tool: name.to_string(),
            detail: source.to_string(),
        }),
    }
}

/// The in-band decline text a refused invocation carries as its
/// `is_error` `tool_result` — the model reads why and steps on, exactly
/// like the grant decline ([`super::refusal`]). No result envelope: no
/// child ran, so there is no exit code and none is invented (§3.3).
pub(in crate::prompt::dispatch) fn refusal_text(tool: &str, reason: &str) -> String {
    format!(
        "{tool:?} was refused by the workflow's tool control (ARCH §3.3 Tool control): {reason}"
    )
}
