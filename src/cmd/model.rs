//! `litany model` — the per-agent model override (ARCH §4.3 *The model
//! mark*, bl-246f).
//!
//! A role's model binding — its `(provider row, model id)` pointer and
//! the `effort` / `priority` knobs beside it — lives on the followed
//! config commit and reaches every conversation on the lineage at once
//! (§2.2, bl-403b). This verb is that fact's per-agent override: it
//! writes the **standing** mark `refs/litany/model/<agent>`
//! ([`crate::workspace::model_mark`]) carrying the binding the one named
//! agent should run on from its next step boundary, and resolution
//! overlays it on the role's `roles:` entry at every hop — so writing
//! the ref *is* the switch, with no re-fork, no rebase and no branch
//! written. Everything the mark does not carry keeps following the
//! lineage. `--clear` deletes the mark, which deletes config, never code
//! (`docs/PRINCIPLES.md` severability). Contrast `litany config`, whose
//! write of `providers.yaml` is the workspace-wide pick, and `litany
//! retarget`, which moves the whole config by re-forking the branch.
//!
//! **The shape is `litany workflow`'s** (§6, bl-5c02): naming a target
//! is what makes an invocation a write; `--clear` is the other write;
//! **bare, the verb READS** — one line on stdout naming the binding the
//! agent's next step will run on and whether a mark or the followed
//! lineage's `roles:` entry answered it. The read is the resolver's own
//! derivation ([`crate::prompt::resolve`]'s overlay) and not a second
//! spelling of it, so it cannot drift from what actually resolves.
//!
//! **Every refusal precedes the mark** (the retarget discipline): the
//! workspace and agent exist, and the agent's role has a `roles:` entry
//! on its followed commit to overlay — a pick never makes an undeclared
//! role dispatchable. Provider-row and model-id validity are **not**
//! checked here: they are brazen's facts, caught at the first live model
//! call exactly as a `providers.yaml` edit's are (§4.2). A declined
//! invocation leaves no debris. The write's one product on stdout is
//! nothing (§3.4); the confirmation rides stderr, because the verb's
//! whole effect is a ref the operator cannot otherwise see.

use super::{Error, Fx, Outcome};
use crate::config::{Effort, PerRepoProviders};
use crate::template::{GitRunner, RealGit};
use crate::workspace::model_mark::{self, ModelMark};
use crate::workspace::{self, current_config};
use std::path::{Path, PathBuf};

/// Why the verb requires the agent to exist, for the shared
/// [`workspace::require_agent`] decline (§2.3).
const REASON: &str = "a model pick marks a running agent to run on another binding (ARCH §4.3)";

/// The control file the overlay is read against — the resolver's own
/// path, spelled here as the verb's read.
const PROVIDERS_FILE: &str = "providers.yaml";

/// `litany model <workspace> <agent> [<provider> <model> [--effort <level>] [--priority] | --clear]`.
#[derive(clap::Args, Debug)]
pub struct Args {
    /// Path to the workspace root.
    pub workspace: PathBuf,
    /// Agent id (== branch name / hyphenated descent) to mark.
    pub agent: String,
    /// brazen provider row the agent's model calls should go through.
    /// Named together with `model`, and naming them is what makes this
    /// invocation a *write*: bare, the verb reads (module docs).
    #[arg(requires = "model", conflicts_with = "clear")]
    pub provider: Option<String>,
    /// Model id on that row — rides the canonical request verbatim.
    #[arg(requires = "provider", conflicts_with = "clear")]
    pub model: Option<String>,
    /// Reasoning-effort level for the agent's calls (§4.3 `effort:`);
    /// omitted means none requested, the provider's default.
    #[arg(long, value_parser = parse_effort, requires = "model")]
    pub effort: Option<Effort>,
    /// Ask the provider's priority lane (§4.3 `priority:`).
    #[arg(long, requires = "model")]
    pub priority: bool,
    /// Remove the mark: the role's own `roles:` entry on the followed
    /// config commit binds the model again from the agent's next step.
    #[arg(long)]
    pub clear: bool,
}

/// The closed `effort` vocabulary at the command line — the config's
/// own words (ARCH §4.3), read through the same serde the file is.
fn parse_effort(word: &str) -> Result<Effort, String> {
    serde_yaml_ng::from_str(word)
        .map_err(|_| format!("effort must be one of low, medium, high — got {word:?}"))
}

/// Pre-flight, then mark, clear or read (§3.4).
pub fn run(args: Args, _fx: &mut Fx) -> Result<Outcome, Error> {
    let e = |source: &dyn std::fmt::Display| Error::new("model", source);
    crate::name::require_agent_id(&args.agent).map_err(|s| e(&s))?;
    let git = RealGit::new();
    workspace::require(&args.workspace).map_err(|s| e(&s))?;
    workspace::require_agent(&args.workspace, &args.agent, REASON, &git).map_err(|s| e(&s))?;
    if args.clear {
        model_mark::clear(&args.workspace, &args.agent, &git).map_err(|s| e(&s))?;
        eprintln!(
            "litany: model mark cleared for [{}] — its role's own binding on the followed \
             config governs from its next step (ARCH §4.3)",
            args.agent,
        );
        return Ok(Outcome::Quiet);
    }
    let (Some(provider), Some(model)) = (args.provider, args.model) else {
        // Neither a target nor --clear: the read. Nothing is written.
        return show(&args.workspace, &args.agent, &git)
            .map(Outcome::Line)
            .map_err(|s| e(&s));
    };
    let (role, _) = lineage_binding(&args.workspace, &args.agent, &git).map_err(|s| e(&s))?;
    let mark = ModelMark {
        provider,
        model,
        effort: args.effort,
        priority: args.priority.then_some(true),
    };
    model_mark::write(&args.workspace, &args.agent, &mark, &git).map_err(|s| e(&s))?;
    eprintln!(
        "litany: [{}] marked to run {} / {} as {role} — it governs from the agent's next step \
         boundary on, and stands until re-marked or cleared (ARCH §4.3)",
        args.agent, mark.provider, mark.model,
    );
    Ok(Outcome::Quiet)
}

/// The agent's role and that role's assignment on its **followed**
/// config commit (§2.2) — the thing a mark overlays. Refused when the
/// role has no entry: the pre-flight that keeps a pick from standing in
/// for a declaration (§4.3).
fn lineage_binding(
    ws: &Path,
    agent: &str,
    git: &dyn GitRunner,
) -> Result<(String, crate::config::per_repo_providers::RoleAssignment), String> {
    let rev = workspace::agent_ref(agent);
    let followed = current_config::current_config(ws, &rev, git).map_err(|s| s.to_string())?;
    let commit = followed.commit();
    let role = crate::prompt::role::derive(&workspace::repo_git(ws), &rev, agent, git)
        .map_err(|s| s.to_string())?
        .unwrap_or_else(|| crate::prompt::WORKER_ROLE.to_string());
    let raw = workspace::show_control(ws, commit, PROVIDERS_FILE, git)
        .map_err(|s| format!("{commit}:{PROVIDERS_FILE}: {s}"))?;
    let per_repo =
        PerRepoProviders::parse(&raw, &PathBuf::from(format!("{commit}:{PROVIDERS_FILE}")))
            .map_err(|s| s.to_string())?;
    let assignment = per_repo.roles.get(&role).cloned().ok_or_else(|| {
        format!(
            "{commit}:{PROVIDERS_FILE} declares no {role:?} role — a model mark overlays a \
             role's binding and cannot stand in for one (ARCH §4.3)"
        )
    })?;
    Ok((role, assignment))
}

/// Answer *which binding governs this agent's model calls* in one line —
/// the verb's read, and its only product on stdout. The derivation is
/// the resolver's: the followed commit's entry, overlaid by the standing
/// mark when one stands.
fn show(ws: &Path, agent: &str, git: &dyn GitRunner) -> Result<String, String> {
    let (role, lineage) = lineage_binding(ws, agent, git)?;
    let mark = model_mark::read(ws, agent, git).map_err(|s| s.to_string())?;
    let (provider, model, effort, priority, origin) = match &mark {
        Some(m) => (
            m.provider.as_str(),
            m.model.as_str(),
            m.effort,
            m.priority,
            format!("marked on [{agent}]"),
        ),
        None => (
            lineage.provider.as_str(),
            lineage.model.as_str(),
            lineage.effort,
            lineage.priority,
            "followed from its governing lineage".to_string(),
        ),
    };
    let knobs = knobs(effort, priority);
    Ok(format!(
        "{agent} runs {provider} / {model}{knobs} as {role} — {origin}"
    ))
}

/// The two knobs as a parenthetical, or nothing when neither is set —
/// the general path with empty inputs renders as the absence it is.
fn knobs(effort: Option<Effort>, priority: Option<bool>) -> String {
    let mut parts = Vec::new();
    if let Some(level) = effort {
        parts.push(format!(
            "effort {}",
            serde_yaml_ng::to_string(&level)
                .unwrap_or_default()
                .trim()
                .to_owned()
        ));
    }
    if priority == Some(true) {
        parts.push("priority".to_string());
    }
    if parts.is_empty() {
        String::new()
    } else {
        format!(" ({})", parts.join(", "))
    }
}
