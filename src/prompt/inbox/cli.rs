//! The `litany message` verb's orchestration (ARCH §2.11, §3.4):
//! deposit, then probe-and-launch, plus the CLI entry that guards the
//! recipient and resolves the sender.
//!
//! Split out of [`super`] at bl-6a7c. Everything here is *wiring* — the
//! order of two acts, the guards before them, and where each
//! process-global arrives from — with no inbox fact of its own; the
//! facts are [`super`]'s paths and [`super::launch`]'s decision.

use super::{
    DepositError, USER_SENDER, deposit,
    launch::{AdvanceLauncher, Launcher, ProbeOutcome, probe_and_launch},
};
use crate::prompt::{Clock, SystemClock};
use std::ffi::OsStr;
use std::io;
use std::path::Path;

/// Every way [`cli_message`] can fail.
#[derive(Debug, thiserror::Error)]
pub enum MessageError {
    #[error(transparent)]
    Deposit(#[from] DepositError),
    #[error(transparent)]
    Layout(#[from] crate::workspace::LayoutError),
    #[error("probe executor lock: {0}")]
    Probe(#[source] io::Error),
    /// The recipient has no `agents/*` ref — the shared existence
    /// decline ([`crate::workspace::require_agent`]). A message is
    /// addressed to an *existing* agent (§2.11), so a deposit no drain
    /// would ever come for is declined rather than made silently into a
    /// directory nothing will read.
    #[error(transparent)]
    UnknownAgent(#[from] crate::workspace::UnknownAgent),
}

/// The `litany message <workspace> <agent> <content>` verb (§2.11,
/// §3.4): deposit, then probe-and-launch. `sender` is resolved by the
/// caller — [`resolve_cli_sender`] for the bin, the calling agent's id
/// for the `message` tool — never from model input. Returns the probe
/// outcome so the caller can report whether a driver was launched.
pub fn cli_message(
    workspace: &Path,
    agent_id: &str,
    content: &str,
    sender: &str,
    clock: &dyn Clock,
    launcher: &dyn Launcher,
) -> Result<ProbeOutcome, MessageError> {
    deposit(workspace, agent_id, sender, content, clock)?;
    probe_and_launch(workspace, agent_id, launcher).map_err(MessageError::Probe)
}

/// CLI entry for `litany message <workspace> <agent> <content>` (§3.4).
/// Guards the recipient first — the layout (§2.2) and then the agent's
/// existence ([`crate::workspace::agent_exists`]): §2.11 addresses a
/// message to an *existing* agent, so a deposit that no drain could ever
/// come for is declined loudly rather than written into a directory that
/// nothing will ever read. Returns the probe outcome so the verb layer
/// can advise on a branch whose latest model call failed (§2.10).
/// Kept in the lib so the bin stays under the 300-line cap and the wiring
/// is unit-testable — the same discipline as `stop::cli_run`. Resolves
/// the sender from `conv_branch` ([`resolve_cli_sender`]) and wires the
/// production clock plus the real [`AdvanceLauncher`] detached spawn
/// (§2.11) at `driver_target`. **Both arrive from the binding** — each
/// is a process-global, and neither is reached for here
/// (`cmd::Fx::driver_target` and `cmd::Fx::conv_branch`, §3.4, whose
/// own doc records what reading the second one *here* cost, bl-b5b1).
pub fn cli_run(
    workspace: &Path,
    agent: &str,
    content: &str,
    conv_branch: Option<&OsStr>,
    driver_target: &Path,
) -> Result<ProbeOutcome, MessageError> {
    crate::workspace::require(workspace)?;
    crate::workspace::require_agent(
        workspace,
        agent,
        "a message is addressed to an existing agent, by id or unique name (ARCH §2.11)",
        &crate::template::RealGit::new(),
    )?;
    let sender = resolve_cli_sender(conv_branch);
    let launcher = AdvanceLauncher::with_exe(driver_target.to_path_buf());
    cli_message(workspace, agent, content, &sender, &SystemClock, &launcher)
}

/// Resolve the deposit sender for a direct `litany message` invocation
/// from the `LITANY_CONV_BRANCH` value (§3.3): the calling agent's id
/// when the harness set it (an agent's `message` tool re-entering the
/// verb), else [`USER_SENDER`] for a bare user/frontend invocation. An
/// unset *or empty* value is `user`, mirroring the `LITANY_HOME`
/// set-and-non-empty discipline (§2.2).
pub fn resolve_cli_sender(branch_env: Option<&OsStr>) -> String {
    match branch_env {
        Some(v) if !v.is_empty() => v.to_string_lossy().into_owned(),
        _ => USER_SENDER.to_string(),
    }
}
