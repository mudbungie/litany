//! `litany replay` — reconstruct a scratch workspace under `LITANY_HOME`
//! from an archive and print its path (ARCH §9.2).

use super::new::path_line;
use super::{Error, Fx, Outcome};
use std::path::PathBuf;

/// `litany replay <archive>`.
#[derive(clap::Args, Debug)]
pub struct Args {
    /// Directory `litany bundle` wrote the archive into.
    pub archive: PathBuf,
}

/// Replay into a scratch workspace and print its path — the verb's one
/// product (§3.4). The `Path`→line render is the shared [`path_line`]
/// (covered via `new`), so the success arm carries no test-only region.
pub fn run(args: Args, _fx: &mut Fx) -> Result<Outcome, Error> {
    crate::archive::replay_cli(&args.archive)
        .map(path_line)
        .map_err(|e| Error::new("replay", e))
}
