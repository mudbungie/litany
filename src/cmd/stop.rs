//! `litany stop` — SIGTERM a conversation branch's executor pgid (ARCH
//! §2.9). Idempotent for already-stopped branches; `--stop-children`
//! walks the id namespace to reach descendants.

use super::{Error, Fx, Outcome};
use crate::prompt::stop;
use std::path::PathBuf;

/// `litany stop <repo> <branch> [--stop-children]`.
#[derive(clap::Args, Debug)]
pub struct Args {
    /// Path to the workspace (conversation repo) root.
    pub repo: PathBuf,
    /// Agent id (== branch name) whose executor to signal.
    pub branch: String,
    /// Also stop the agent's whole subagent subtree (§2.9).
    #[arg(long)]
    pub stop_children: bool,
}

/// Signal the pgid(s) — product-less on success (§3.4).
pub fn run(args: Args, _fx: &mut Fx) -> Result<Outcome, Error> {
    crate::name::require_agent_id(&args.branch).map_err(|e| Error::new("stop", e))?;
    stop::cli_run(&args.repo, &args.branch, args.stop_children)
        .map_err(|e| Error::new("stop", e))?;
    Ok(Outcome::Quiet)
}
