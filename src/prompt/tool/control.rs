//! The **tool control** client (ARCH §3.3 *Tool control*): consult the
//! configured adjudicator about one tool invocation and read its verdict.
//!
//! A control is an external binary the governing `workflow.yaml` names
//! (`tool_control:` — [`crate::config::ToolControl`]). The harness ships
//! **no** control; this module is only the seam's wire contract:
//!
//! - **stdin** — one JSON object ([`ControlRequest`]): the `tool_use`
//!   block verbatim (`id`, `name`, `input`) plus the calling `role` and
//!   `agent_id`. Everything else a control wants — the transcript, the
//!   worktree, an approval file — it reads from disk via the same
//!   `LITANY_CONV_REPO` / `LITANY_CONV_BRANCH` env vars every tool gets.
//! - **stdout** — one JSON verdict ([`Verdict`]): `{"verdict":"pass"}`,
//!   `{"verdict":"refuse","reason":…}` or `{"verdict":"hold","reason":…}`.
//! - **exit 0** — anything else is a control failure, and a control
//!   failure **fails closed** ([`ControlError`], mapped by the seam):
//!   the invocation does not execute. A control is an adjudicator, not
//!   an actor: it must be side-effect-free per consult, because a held
//!   invocation is re-adjudicated on every subsequent drive (§3.3).
//!
//! The control runs in the **workspace root**, not the agent's cwd — it
//! acts for the operator, so the agent's own `cd` must not move it —
//! and under the executor's stop cascade ([`super::subprocess`]): a
//! control felled by the harness's own group SIGTERM mid-stop is the
//! stop, not a fault, classified by the seam exactly as for tools.

use super::subprocess::{ETXTBSY_RETRY_ATTEMPTS, SpawnArgs, spawn_and_capture};
use super::{DEFAULT_TOOL_DEADLINE, ENV_CONV_BRANCH, ENV_CONV_REPO, ExecError};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::ffi::OsString;
use std::os::unix::process::ExitStatusExt;
use std::path::Path;
use std::sync::atomic::AtomicBool;
use thiserror::Error;

/// What the seam tells the control: the invocation as the model emitted
/// it, plus who is asking. Serialized to the control's stdin.
#[derive(Debug, Serialize)]
pub struct ControlRequest<'a> {
    /// `tool_use.id` — what a hold parks on ([`crate::workspace::hold`]).
    pub id: &'a str,
    /// Tool name as the model spelled it.
    pub name: &'a str,
    /// `tool_use.input`, verbatim.
    pub input: &'a Value,
    /// The calling agent's role (§4.3) — scoping by role is the
    /// control's business, not the harness's.
    pub role: &'a str,
    /// The calling agent's id (== `LITANY_CONV_BRANCH`).
    pub agent_id: &'a str,
}

/// The control's answer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Verdict {
    /// The invocation proceeds to the executor unchanged.
    Pass,
    /// The invocation never executes; `reason` reaches the model as an
    /// in-band `is_error` `tool_result` (the grant-decline idiom, §3.3).
    Refuse { reason: String },
    /// The invocation parks before execution for out-of-band review
    /// (§3.3 *Tool control* — the hold mark, re-adjudicated on the next
    /// drive). `reason` is for the operator; the model sees nothing.
    Hold { reason: String },
}

/// Every way a consult can fail. All of these fail closed at the seam
/// — the invocation does not execute — except a kill by the harness's
/// own stop SIGTERM, which the seam classifies by the stop flag.
#[derive(Debug, Error)]
pub enum ControlError {
    /// The control binary would not spawn (missing, permission, …).
    #[error("spawn control {command:?}: {source}")]
    Spawn {
        command: String,
        #[source]
        source: std::io::Error,
    },
    /// The control died from a signal. With the stop flag set this is
    /// the §2.9 cascade (the seam reads it as the stop); without, a
    /// control crash — failed closed like every other control fault.
    #[error("control {command:?} terminated by signal {signal}")]
    KilledBySignal { command: String, signal: i32 },
    /// The control ran but broke the protocol: a non-zero exit, or
    /// stdout that is not one JSON [`Verdict`].
    #[error("control {command:?} broke the verdict protocol: {detail}")]
    Protocol { command: String, detail: String },
}

/// Spawn `command` per the module contract and parse its verdict.
/// `conv_repo` is the workspace root — the control's cwd and its
/// `LITANY_CONV_REPO`; `stop` is the executor's §2.9 flag, so a stop
/// landing mid-consult cascades onto the control like any tool.
pub fn consult(
    command: &str,
    request: &ControlRequest<'_>,
    conv_repo: &Path,
    stop: &AtomicBool,
) -> Result<Verdict, ControlError> {
    let stdin = serde_json::to_vec(request).expect("ControlRequest serializes");
    let binary = OsString::from(command);
    let extra_env = [
        (ENV_CONV_REPO, conv_repo.as_os_str().to_owned()),
        (ENV_CONV_BRANCH, OsString::from(request.agent_id)),
    ];
    let captured = spawn_and_capture(&SpawnArgs {
        binary: &binary,
        args: &[],
        stdin_bytes: &stdin,
        extra_env: &extra_env,
        cwd: conv_repo,
        stop,
        deadline: DEFAULT_TOOL_DEADLINE,
        etxtbsy_budget: ETXTBSY_RETRY_ATTEMPTS,
        tool_name: command,
    })
    .map_err(|e| spawn_fault(command, e))?;
    let Some(code) = captured.status.code() else {
        return Err(ControlError::KilledBySignal {
            command: command.to_string(),
            signal: captured.status.signal().unwrap_or(0),
        });
    };
    if code != 0 {
        return Err(ControlError::Protocol {
            command: command.to_string(),
            detail: format!(
                "exited {code}; stderr: {}",
                String::from_utf8_lossy(&captured.stderr).trim()
            ),
        });
    }
    parse_verdict(&captured.stdout).map_err(|detail| ControlError::Protocol {
        command: command.to_string(),
        detail: format!(
            "stdout is not a verdict ({detail}): {}",
            String::from_utf8_lossy(&captured.stdout).trim()
        ),
    })
}

/// Map a capture failure onto the control taxonomy. `spawn_and_capture`
/// fails only at the spawn ([`super::subprocess`]); the fold of any
/// other executor error into the protocol fault keeps the map total
/// without inventing cases — and fails closed either way.
fn spawn_fault(command: &str, e: ExecError) -> ControlError {
    match e {
        ExecError::Spawn { source, .. } => ControlError::Spawn {
            command: command.to_string(),
            source,
        },
        other => ControlError::Protocol {
            command: command.to_string(),
            detail: other.to_string(),
        },
    }
}

/// Parse the stdout verdict strictly. A raw struct rather than a tagged
/// serde enum because internal tagging ignores `deny_unknown_fields`,
/// and the protocol must fail closed on a field it does not understand
/// (a control that thinks it can rewrite the input must learn otherwise
/// loudly, not by silent omission).
fn parse_verdict(stdout: &[u8]) -> Result<Verdict, String> {
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Raw {
        verdict: String,
        #[serde(default)]
        reason: Option<String>,
    }
    let raw: Raw = serde_json::from_slice(stdout).map_err(|e| e.to_string())?;
    match (raw.verdict.as_str(), raw.reason) {
        ("pass", None) => Ok(Verdict::Pass),
        ("pass", Some(_)) => Err("a pass carries no reason".into()),
        ("refuse", Some(reason)) => Ok(Verdict::Refuse { reason }),
        ("hold", Some(reason)) => Ok(Verdict::Hold { reason }),
        (v @ ("refuse" | "hold"), None) => Err(format!("{v:?} requires a reason")),
        (other, _) => Err(format!("unknown verdict {other:?}")),
    }
}

#[cfg(test)]
mod tests;
