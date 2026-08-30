//! `litany config` — author a config commit beyond `litany new` (ARCH
//! §2.2, §2.3): the only act besides `new` that advances a config
//! branch. The interactive `$EDITOR` hand-off arrives through
//! [`Fx::editor`](super::Fx::editor); everything else lives in
//! [`crate::template::authoring::from_cli`].

use super::{Error, Fx, Outcome};
use crate::harness_root;
use crate::template::authoring::Pass;
use crate::template::{self, RealGit};
use std::path::PathBuf;

/// `litany config <workspace> [<name>] [--from <source>] [--orphan]`.
#[derive(clap::Args, Debug)]
pub struct Args {
    /// Path to the workspace (conversation repo) root.
    pub workspace: PathBuf,
    /// Config branch to author on (`config/<name>`); defaults to `default`.
    pub name: Option<String>,
    /// Fork a new branch off `config/<source>` instead of advancing.
    #[arg(long)]
    pub from: Option<String>,
    /// Start a fresh orphan lineage instead of advancing.
    #[arg(long)]
    pub orphan: bool,
}

/// Materialize, edit via [`Fx::editor`](super::Fx::editor), and commit —
/// product-less when the commit lands (§3.4). A **declined pass** — an
/// edit that changed nothing — is not a failure: it exits 0 and reports
/// the branch that did not move as the verb's one stdout line, which is
/// also the machine-readable signal (empty stdout = a commit landed).
/// Failures — root resolution or the authoring pass — carry the `config`
/// prefix through one conversion.
pub fn run(args: Args, fx: &mut Fx) -> Result<Outcome, Error> {
    go(args, fx).map_err(|e| Error::new("config", e))
}

fn go(args: Args, fx: &mut Fx) -> Result<Outcome, Box<dyn std::error::Error>> {
    let roots = harness_root::resolve()?;
    let pass = template::authoring::from_cli(
        &args.workspace,
        &roots.data,
        args.name.as_deref(),
        args.from.as_deref(),
        args.orphan,
        fx.editor,
        &RealGit::new(),
    )?;
    Ok(match pass {
        Pass::Landed => Outcome::Quiet,
        Pass::Declined { target } => Outcome::Line(format!(
            "{target} unchanged: the edit changed nothing, so no config commit was authored"
        )),
    })
}
