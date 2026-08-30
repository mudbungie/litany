//! `litany delete` — remove an agent and every slice of it (ARCH §9.2
//! *Retention and GC*). `bundle` composes in front: bundle-then-delete
//! is the archive path, and this verb archives nothing itself.

use super::{Error, Fx, Outcome};
use crate::template::RealGit;
use std::path::PathBuf;

/// `litany delete <workspace> <agent> [--children] [--dry-run]`.
#[derive(clap::Args, Debug)]
pub struct Args {
    /// Path to the workspace (conversation repo) root.
    pub workspace: PathBuf,
    /// Agent id (== branch name) to remove.
    pub agent: String,
    /// Also remove the agent's whole descent subtree (§2.3). Without
    /// it, an agent with descendants is declined naming them.
    #[arg(long)]
    pub children: bool,
    /// Report what would be removed and remove nothing — the plan a
    /// caller's confirmation enumerates (§3.5).
    #[arg(long)]
    pub dry_run: bool,
}

/// Remove it (or plan it) and print the census — the verb's one product
/// (§3.4).
pub fn run(args: Args, _fx: &mut Fx) -> Result<Outcome, Error> {
    crate::name::require_agent_id(&args.agent).map_err(|e| Error::new("delete", e))?;
    let report = crate::archive::delete(
        &args.workspace,
        &args.agent,
        args.children,
        args.dry_run,
        &RealGit::new(),
    )
    .map_err(|e| Error::new("delete", e))?;
    Ok(Outcome::Line(report.to_string()))
}
