//! `litany stop` — SIGTERM a conversation branch's executor pgid and
//! every descendant executor's (ARCH §2.9). Idempotent for
//! already-stopped branches.

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
    /// Retired (§2.9, bl-3114): every stop walks the subagent subtree,
    /// so this changes nothing. Kept because the boundary above litany
    /// spells it and a verb does not break its callers to retire a word.
    #[arg(long)]
    pub stop_children: bool,
}

/// Signal the pgid(s) — product-less on success (§3.4). The subtree walk
/// is unconditional, so [`Args::stop_children`] is read by nothing: it is
/// a spelling the CLI still accepts, not a mode (§2.9).
pub fn run(args: Args, _fx: &mut Fx) -> Result<Outcome, Error> {
    crate::name::require_agent_id(&args.branch).map_err(|e| Error::new("stop", e))?;
    stop::cli_run(&args.repo, &args.branch).map_err(|e| Error::new("stop", e))?;
    Ok(Outcome::Quiet)
}
