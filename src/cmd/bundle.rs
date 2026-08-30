//! `litany bundle` — archive an agent subtree as one `git bundle` plus
//! the `steps/`/`inbox/` slices (ARCH §9.2).

use super::{Error, Fx, Outcome};
use crate::template::RealGit;
use std::path::PathBuf;

/// `litany bundle <workspace> <agent> <out-dir>`.
#[derive(clap::Args, Debug)]
pub struct Args {
    /// Path to the workspace (conversation repo) root.
    pub workspace: PathBuf,
    /// Agent id (== branch name) whose subtree to archive.
    pub agent: String,
    /// Directory to write the archive into.
    pub out_dir: PathBuf,
}

/// Write the bundle — product-less on success (§3.4).
pub fn run(args: Args, _fx: &mut Fx) -> Result<Outcome, Error> {
    crate::name::require_agent_id(&args.agent).map_err(|e| Error::new("bundle", e))?;
    crate::archive::bundle(&args.workspace, &args.agent, &args.out_dir, &RealGit::new())
        .map_err(|e| Error::new("bundle", e))?;
    Ok(Outcome::Quiet)
}
