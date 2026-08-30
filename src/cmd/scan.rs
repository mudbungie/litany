//! `litany scan` — the operator sweep-and-flush (ARCH §2.11, §8).
//! Hand/cron only; never wired into any driver hot path.

use super::{Error, Fx, Outcome};
use crate::prompt::inbox::scan;
use std::path::PathBuf;

/// `litany scan <workspace>`.
#[derive(clap::Args, Debug)]
pub struct Args {
    /// Path to the workspace (conversation repo) root to sweep.
    pub workspace: PathBuf,
}

/// Run one workspace-wide sweep-and-flush and print the report — the
/// verb's one product (§3.4). The detached-launch target is
/// [`Fx::driver_target`](super::Fx::driver_target).
pub fn run(args: Args, fx: &mut Fx) -> Result<Outcome, Error> {
    let report =
        scan::cli_run(&args.workspace, &fx.driver_target).map_err(|e| Error::new("scan", e))?;
    Ok(Outcome::Line(report.to_string()))
}
