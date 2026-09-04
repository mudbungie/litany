//! `litany proposal` — the operator's half of the learning loop
//! (`docs/DESIGN_LEARNING_LOOP.md` §3 *The operator verb*, ARCH §3.4).
//!
//! **One verb, modes by argument**, the `litany workflow` shape: bare it
//! lists every staged proposal, an id shows one whole, and `--accept` /
//! `--reject` act on the one named. There is no `list` subcommand and no
//! `--list` flag, because the argument already says which question is
//! being asked.
//!
//! **Nothing here derives anything.** The queries and the two writes are
//! [`crate::workspace::proposal`]'s, where the ref arithmetic and the
//! compare-and-swap live; this module resolves the mode, converts one
//! error voice and hands back one product (§3.4).
//!
//! **`--accept` needs an id and says so.** A verb that accepted "the
//! only one" would do something different the day a second proposal was
//! staged, which is exactly the class of surprise a destructive act must
//! not have (`docs/PRINCIPLES.md` *Decline illegal operations*).

use super::{Error, Fx, Outcome};
use crate::template::RealGit;
use crate::workspace::proposal;
use std::path::PathBuf;

/// `litany proposal <workspace> [<id>] [--accept | --reject]`.
#[derive(clap::Args, Debug)]
pub struct Args {
    /// Path to the workspace (conversation repo) root.
    pub workspace: PathBuf,
    /// The proposal's id — the reviewer that staged it, which is the
    /// branch name `proposal/<id>`. Omitted, the verb lists them all.
    pub id: Option<String>,
    /// Fast-forward the proposal's lineage onto it and delete the
    /// branch. Refused when the lineage advanced since the review read
    /// it, naming the tip it now stands at.
    #[arg(long, conflicts_with = "reject")]
    pub accept: bool,
    /// Delete the proposal branch. The reviewer's own branch survives as
    /// the record of its reasoning.
    #[arg(long)]
    pub reject: bool,
}

/// Resolve the mode from the arguments and perform it — one product on
/// stdout in every mode (§3.4): the table, the proposal, or the line
/// naming what moved.
pub fn run(args: Args, _fx: &mut Fx) -> Result<Outcome, Error> {
    let e = |source: &dyn std::fmt::Display| Error::new("proposal", source);
    let git = RealGit::new();
    let ws = &args.workspace;
    let Some(id) = args.id.as_deref() else {
        if args.accept || args.reject {
            return Err(e(
                &"name the proposal to act on: `litany proposal <workspace> <id> --accept|--reject`",
            ));
        }
        let rows = proposal::list(ws, &git).map_err(|s| e(&s))?;
        return Ok(Outcome::Line(proposal::render(&rows)));
    };
    crate::name::require_agent_id(id).map_err(|s| e(&s))?;
    let product = match (args.accept, args.reject) {
        (true, _) => proposal::accept(ws, id, &git),
        (_, true) => proposal::reject(ws, id, &git),
        _ => proposal::show(ws, id, &git),
    };
    Ok(Outcome::Line(product.map_err(|s| e(&s))?))
}
