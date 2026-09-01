//! `litany workflow` — switch which workflow governs an agent (ARCH §6
//! *The workflow mark*, `docs/DESIGN_WORKFLOW_SWITCH.md`).
//!
//! The workflow — the config's `workflow.yaml`, the named declaration of
//! what happens at every step (§6) — is frozen at fork like every other
//! control fact (§2.2), and this verb is the workflow fact's own exit
//! (operator ruling 2026-08-31): it writes the **standing** mark
//! `refs/litany/workflow/<agent>` at the named lineage's head
//! ([`crate::workspace::workflow_mark`]). Resolution consults the mark
//! fresh at every step boundary — nearest mark on the agent's descent
//! wins — so the switch is effective at the agent's next step with no
//! re-fork, no rebase and no migration; `--clear` deletes the mark,
//! which deletes config, never code (`docs/PRINCIPLES.md` severability).
//! Contrast `litany retarget` (§2.2), which moves the *whole* config by
//! re-forking the branch; this verb moves the workflow fact alone and
//! writes no branch at all.
//!
//! **Every refusal precedes the mark** (the retarget discipline): a
//! declined switch leaves no debris. The one product on stdout is
//! nothing (§3.4) — a mark is not a value the caller composes with; the
//! confirmation rides stderr, because the verb's whole effect is a ref
//! the operator cannot otherwise see, taking effect at a moment they
//! did not choose.

use super::{Error, Fx, Outcome};
use crate::config::Workflow;
use crate::config::version::Version;
use crate::template::{GitRunner, RealGit};
use crate::workspace::{self, DEFAULT_CONFIG_NAME, workflow_mark};
use std::path::{Path, PathBuf};

/// Why the verb requires the agent to exist, for the shared
/// [`workspace::require_agent`] decline (§2.3).
const REASON: &str =
    "a workflow switch marks a running agent to resolve another workflow (ARCH §6)";

/// The §10 schema-version control file, spelled here as the verb's own
/// read; the resolver reads the same path (`prompt::resolve`).
const VERSION_FILE: &str = "version";

/// `litany workflow <workspace> <agent> [--config <name> | --clear]`.
#[derive(clap::Args, Debug)]
pub struct Args {
    /// Path to the workspace root.
    pub workspace: PathBuf,
    /// Agent id (== branch name / hyphenated descent) to mark.
    pub agent: String,
    /// Config lineage whose head's `workflow.yaml` should govern the
    /// agent from its next step boundary on. Defaults to `default` —
    /// the general path with empty inputs, the same reading every other
    /// config-naming verb gives an unnamed config.
    #[arg(long, conflicts_with = "clear")]
    pub config: Option<String>,
    /// Remove the mark: the governing config commit's workflow governs
    /// again from the agent's next step boundary on.
    #[arg(long)]
    pub clear: bool,
}

/// Pre-flight, then mark (or clear) — product-less on stdout (§3.4).
pub fn run(args: Args, _fx: &mut Fx) -> Result<Outcome, Error> {
    let e = |source: &dyn std::fmt::Display| Error::new("workflow", source);
    crate::name::require_agent_id(&args.agent).map_err(|s| e(&s))?;
    let git = RealGit::new();
    workspace::require(&args.workspace).map_err(|s| e(&s))?;
    workspace::require_agent(&args.workspace, &args.agent, REASON, &git).map_err(|s| e(&s))?;
    if args.clear {
        workflow_mark::clear(&args.workspace, &args.agent, &git).map_err(|s| e(&s))?;
        eprintln!(
            "litany: workflow mark cleared for [{}] — its governing config's workflow \
             governs from its next step (ARCH §6)",
            args.agent,
        );
        return Ok(Outcome::Quiet);
    }
    let name = args.config.as_deref().unwrap_or(DEFAULT_CONFIG_NAME);
    let target = preflight(&args.workspace, name, &git).map_err(|s| e(&s))?;
    workflow_mark::write(&args.workspace, &args.agent, &target, &git).map_err(|s| e(&s))?;
    eprintln!(
        "litany: [{}] marked to run {}'s workflow ({}) — it governs from the agent's next \
         step boundary on, and stands until re-marked or cleared (ARCH §6)",
        args.agent,
        workspace::config_ref(name),
        &target[..target.len().min(12)],
    );
    Ok(Outcome::Quiet)
}

/// Everything the verb refuses **before** the mark is written, returning
/// the target commit: the lineage exists, and its head's `version` (§10)
/// and `workflow.yaml` (the closed §6 vocabulary) parse — so a standing
/// mark always names a commit the resolver can read. Marking is
/// otherwise unconditional: last write wins, and a mark naming the
/// commit already answering the governing question is behaviorally
/// identical to no mark, so no no-op arm exists to drift. The
/// `dispatch(<role>)` cross-check stays at resolution, where the marked
/// workflow meets the governing `providers.yaml` (§4.3).
fn preflight(ws: &Path, name: &str, git: &dyn GitRunner) -> Result<String, String> {
    workspace::require_lineage(ws, name, git).map_err(|s| s.to_string())?;
    let spec = format!("{}^{{commit}}", workspace::config_ref(name));
    let target = git
        .run_capture(&workspace::repo_git(ws), &["rev-parse", &spec])
        .map_err(|s| s.to_string())?
        .trim()
        .to_string();
    let origin = |path: &str| PathBuf::from(format!("{target}:{path}"));
    let version = control(ws, &target, VERSION_FILE, git)?;
    Version::parse(&version, &origin(VERSION_FILE)).map_err(|s| s.to_string())?;
    let workflow = control(ws, &target, crate::prompt::WORKFLOW_FILE, git)?;
    Workflow::parse(&workflow, &origin(crate::prompt::WORKFLOW_FILE)).map_err(|s| s.to_string())?;
    Ok(target)
}

/// One control read off the target commit, labelled with the control
/// file's one true address (§2.2).
fn control(ws: &Path, commit: &str, path: &str, git: &dyn GitRunner) -> Result<String, String> {
    workspace::show_control(ws, commit, path, git).map_err(|s| format!("{commit}:{path}: {s}"))
}
