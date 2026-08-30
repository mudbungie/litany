//! `litany advance` — one hop of the §6 workflow chain: take the lease,
//! deliver, step, and hand off the successor exec. The §2.9 preludes
//! (`become_pgid_leader` + `install_stop_handler`) are the binding's, run
//! before [`run`] ([`super::prelude`]); the successor `execve` is the
//! binding's [`Outcome::Exec`](super::Outcome::Exec) act.

use super::{Error, Fx, Outcome};
use crate::prompt::dispatch::advance::cli::{self, AdvanceHandoff};
use std::path::PathBuf;

/// `litany advance <workspace> <agent>`.
#[derive(clap::Args, Debug)]
pub struct Args {
    /// Path to the workspace (conversation repo) root.
    pub workspace: PathBuf,
    /// Agent id (== branch name / hyphenated descent) to drive.
    pub agent: String,
}

/// Run one hop against the binding-injected driver target and stop flag
/// ([`Fx`](super::Fx)); a tools-pending hop yields the prepared successor
/// [`Outcome::Exec`](super::Outcome::Exec), everything else is
/// product-less ([`AdvanceHandoff::Done`] → [`Outcome::Quiet`], §3.4).
pub fn run(args: Args, fx: &mut Fx) -> Result<Outcome, Error> {
    crate::name::require_agent_id(&args.agent).map_err(|e| Error::new("advance", e))?;
    let handoff = cli::cli_run(
        &args.workspace,
        &args.agent,
        &fx.driver_target,
        fx.adapter_target.as_deref(),
        fx.stop,
        fx.tool_injection,
    )
    .map_err(|e| Error::new("advance", e))?;
    Ok(outcome_of(handoff))
}

/// Map a hop's [`AdvanceHandoff`] to the binding's [`Outcome`]: a
/// tools-pending hop hands off the prepared successor to `execve`
/// ([`Outcome::Exec`]); any completed hop is product-less
/// ([`Outcome::Quiet`]). Split out so both arms are unit-covered — the
/// `Exec` arm's value only arises from a real provider-driven hop.
pub(crate) fn outcome_of(handoff: AdvanceHandoff) -> Outcome {
    match handoff {
        AdvanceHandoff::Exec(cmd) => Outcome::Exec(cmd),
        AdvanceHandoff::Done => Outcome::Quiet,
    }
}
