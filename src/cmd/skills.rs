//! `litany skills` — the skill census (`docs/DESIGN_LEARNING_LOOP.md`
//! §5, ARCH §3.3, §3.4).
//!
//! **The curator is a query.** One row per skill both homes offer —
//! name, owner (`pool` or `workspace`), state, and the two dates git
//! already holds: the newest `agents/*` commit that added
//! `skills/<name>/` (the `load_skill` copy *is* the use) and the newest
//! `config/*` commit touching it. Nothing is stored, nothing is
//! counted, and no process is founded to keep any of it; the derivation
//! is [`crate::skill::census`].
//!
//! **Ages, never a horizon.** Rows print git's own relative age and come
//! oldest-used first. A wall-clock "stale" cutoff is policy, policy is
//! config, and this verb adds none — the reader draws the line.
//!
//! **`--config <name>` names the lineage**, defaulting to `default` —
//! the same reading every other config-naming verb gives an unnamed
//! config (`litany workflow`, `litany retarget`). Workspace skills and
//! the archive container live *in* a config commit, so the question
//! "which skills does this workspace have" is asked of one lineage's
//! tip; the install pool is the box's and answers the same everywhere.
//!
//! The table is the verb's one product on stdout (§3.4), headers
//! included: a workspace with no skills at all prints the headers and
//! nothing else — the general path with empty inputs, not an arm.

use super::{Error, Fx, Outcome};
use crate::harness_root;
use crate::skill::census;
use crate::template::{GitRunner, RealGit};
use crate::workspace::{self, DEFAULT_CONFIG_NAME};
use std::path::PathBuf;

/// `litany skills <workspace> [--config <name>]`.
#[derive(clap::Args, Debug)]
pub struct Args {
    /// Path to the workspace (conversation repo) root to census.
    pub workspace: PathBuf,
    /// Config lineage whose tip holds the workspace skills and the
    /// archive container. Defaults to `default`.
    #[arg(long)]
    pub config: Option<String>,
}

/// Derive the census off the named lineage's tip and the install pool,
/// and print it — one product, one conversion of every failure.
pub fn run(args: Args, _fx: &mut Fx) -> Result<Outcome, Error> {
    let e = |source: &dyn std::fmt::Display| Error::new("skills", source);
    let git = RealGit::new();
    workspace::require(&args.workspace).map_err(|s| e(&s))?;
    let name = args.config.as_deref().unwrap_or(DEFAULT_CONFIG_NAME);
    workspace::require_lineage(&args.workspace, name, &git).map_err(|s| e(&s))?;
    let spec = format!("{}^{{commit}}", workspace::config_ref(name));
    let commit = git
        .run_capture(&workspace::repo_git(&args.workspace), &["rev-parse", &spec])
        .map_err(|s| e(&s))?
        .trim()
        .to_owned();
    let roots = harness_root::resolve().map_err(|s| e(&s))?;
    let rows = census::census(&args.workspace, &commit, &roots.data, &git);
    Ok(Outcome::Line(census::render(&rows)))
}
