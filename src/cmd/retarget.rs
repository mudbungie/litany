//! `litany retarget` — the change of config lineage (ARCH §2.2, §3.4).
//!
//! Fork chooses the lineage, and resolution follows that lineage's tip
//! by itself (§2.2, bl-403b) — so what this verb changes is *which
//! lineage* an agent follows (or, where diverged lineages left the
//! agent held on its fork commit, which one settles it). It writes no
//! branch. It writes a **ref
//! mark**, `refs/litany/retarget/<agent-id>`, at the target config commit
//! ([`crate::workspace::retarget`]); the agent's own executor consumes it
//! at its next `advance` step boundary and lands the re-fork there
//! ([`crate::prompt::retarget`]). §2.3's branch-advancement invariant is
//! untouched: the user marks, the executor writes.
//!
//! **Every refusal precedes the mark**, so a declined retarget leaves no
//! debris at all — the same validity-before-fork discipline the §6 budget
//! gate and the §3.3 descriptor check already hold to at every fork.
//!
//! The one product on stdout is nothing (§3.4): a mark is not a value the
//! caller composes with. The confirmation rides stderr, like `prime`'s
//! (bl-7e9e) — this verb's whole effect is a ref the operator did not name
//! and cannot see, taking effect at a moment they did not choose, so a
//! silent success would leave them unable to tell it from a no-op.

use super::{Error, Fx, Outcome};
use crate::prompt::retarget::preflight;
use crate::template::RealGit;
use crate::workspace::{self, DEFAULT_CONFIG_NAME};
use std::path::PathBuf;

/// `litany retarget <workspace> <agent> [--config <name>]`.
#[derive(clap::Args, Debug)]
pub struct Args {
    /// Path to the workspace (conversation repo) root.
    pub workspace: PathBuf,
    /// Agent id (== branch name / hyphenated descent) to retarget.
    pub agent: String,
    /// Config lineage whose head the agent should be governed by from its
    /// next step on. Defaults to `default` — the general path with empty
    /// inputs, the same reading `litany prompt` gives an unnamed config.
    #[arg(long)]
    pub config: Option<String>,
}

/// Pre-flight, then mark — product-less on stdout (§3.4).
pub fn run(args: Args, _fx: &mut Fx) -> Result<Outcome, Error> {
    let e = |source: &dyn std::fmt::Display| Error::new("retarget", source);
    crate::name::require_agent_id(&args.agent).map_err(|s| e(&s))?;
    let name = args.config.as_deref().unwrap_or(DEFAULT_CONFIG_NAME);
    let git = RealGit::new();
    let target = preflight(&args.workspace, &args.agent, name, &git).map_err(|s| e(&s))?;
    match target {
        None => eprintln!(
            "litany: {} already governs [{}] — nothing to retarget",
            workspace::config_ref(name),
            args.agent,
        ),
        Some(commit) => {
            workspace::retarget::write(&args.workspace, &args.agent, &commit, &git)
                .map_err(|s| e(&s))?;
            eprintln!(
                "litany: [{}] marked for retarget onto {} ({}); it lands at the agent's next \
                 step (ARCH §2.2)",
                args.agent,
                &commit[..commit.len().min(12)],
                workspace::config_ref(name),
            );
        }
    }
    Ok(Outcome::Quiet)
}
