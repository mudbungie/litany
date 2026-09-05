//! `litany workflow` — switch which workflow governs an agent (ARCH §6
//! *The workflow mark*, `docs/DESIGN_WORKFLOW_SWITCH.md`).
//!
//! The workflow — the config's `workflow.yaml`, the named declaration of
//! what happens at every step (§6) — follows the governing lineage's
//! tip like every other control fact (§2.2, bl-403b), and this verb is
//! the workflow fact's per-agent override (operator ruling 2026-08-31):
//! it writes the **standing** mark `refs/litany/workflow/<agent>` at
//! the named lineage's head ([`crate::workspace::workflow_mark`]),
//! which wins over the followed tip until cleared — a pin as well as a
//! switch. Resolution consults the mark
//! fresh at every step boundary — nearest mark on the agent's descent
//! wins — so the switch is effective at the agent's next step with no
//! re-fork, no rebase and no migration; `--clear` deletes the mark,
//! which deletes config, never code (`docs/PRINCIPLES.md` severability).
//! Contrast `litany retarget` (§2.2), which moves the *whole* config by
//! re-forking the branch; this verb moves the workflow fact alone and
//! writes no branch at all.
//!
//! **Bare, the verb READS** (bl-5c02). `litany workflow <ws> <agent>`
//! with neither `--config` nor `--clear` answers *which `workflow.yaml`
//! governs this agent* on stdout — the derivation
//! ([`crate::prompt::resolve::workflow_source::source_of`]: nearest mark
//! on the descent, else the followed config commit) rendered as one
//! line, with the marked commit's lineage name when a `config/*` ref
//! stands on it. Until then the mark was standing state no read surface
//! reported and an operator's only way to ask was raw git against a ref
//! namespace no verb printed. It is this verb's read and not `litany
//! scan`'s row because scan is an **act** — it sweeps and launches
//! drivers (§2.11) — and asking a policy question must not fork
//! anything; and it is a mode of this verb rather than a new one
//! because the fact already has an owner. **Bare no longer marks.** It
//! used to mean `--config default`, so the gesture that reads most like
//! an inspection silently pinned an agent; the write now names its
//! target, and a default whose removal deletes config rather than code
//! is exactly the one to remove (`docs/PRINCIPLES.md` severability).
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
use crate::prompt::resolve::{ConfigSource, workflow_source};
use crate::template::{GitRunner, RealGit};
use crate::workspace::{self, workflow_mark};
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
    /// agent from its next step boundary on. Naming it is what makes
    /// this invocation a *write*: with neither this nor `--clear` the
    /// verb reads instead (module docs, bl-5c02).
    #[arg(long, conflicts_with = "clear")]
    pub config: Option<String>,
    /// Remove the mark: the followed config commit's workflow governs
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
            "litany: workflow mark cleared for [{}] — its followed config's workflow \
             governs from its next step (ARCH §6)",
            args.agent,
        );
        return Ok(Outcome::Quiet);
    }
    let Some(name) = args.config.as_deref() else {
        // Neither flag: the read (bl-5c02). Nothing is written and no
        // ref is touched — the answer is the verb's one product (§3.4).
        return show(&args.workspace, &args.agent, &git)
            .map(Outcome::Line)
            .map_err(|s| e(&s));
    };
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

/// Answer *which `workflow.yaml` governs this agent* in one line
/// (bl-5c02) — the verb's read mode, and its only product on stdout.
///
/// The derivation is not re-spelled here: `source_of` is the same
/// composition the resolver runs at every step boundary, so the read
/// cannot drift from what actually resolves. What the rendering adds is
/// the two facts an operator cannot see from the sha — **who holds the
/// mark** (an ancestor's mark governs a whole subtree, §6, and the agent
/// asked about may not be the one carrying it) and **which lineage
/// stands on that commit**, when a `config/*` ref does. A commit no
/// config ref points at is ordinary — the lineage has advanced past a
/// mark that deliberately pins an older commit — so it is rendered as
/// the absence it is rather than declined.
fn show(ws: &Path, agent: &str, git: &dyn GitRunner) -> Result<String, String> {
    let rev = workspace::agent_ref(agent);
    let followed =
        workspace::current_config::current_config(ws, &rev, git).map_err(|s| s.to_string())?;
    let source =
        workflow_source::source_of(ws, &ConfigSource::Agent(agent), followed.commit(), git);
    let commit = source.commit();
    let origin = match &source {
        workflow_source::Source::Marked { holder, .. } if holder == agent => {
            format!("marked on [{holder}]")
        }
        workflow_source::Source::Marked { holder, .. } => {
            format!("marked on ancestor [{holder}]")
        }
        workflow_source::Source::Followed { .. } => match followed.held() {
            None => "followed from its governing lineage".to_string(),
            Some(tips) => {
                format!("followed from its fork commit — {tips} diverged config lineages")
            }
        },
    };
    let lineage = match lineage_at(ws, commit, git)? {
        Some(name) => format!(" ({name})"),
        None => String::new(),
    };
    Ok(format!(
        "{agent} runs {}:{}{lineage} — {origin}",
        &commit[..commit.len().min(12)],
        crate::prompt::WORKFLOW_FILE,
    ))
}

/// The `config/*` ref standing exactly on `commit`, if one does — the
/// lineage name an operator names on a command line, recovered from the
/// commit the derivation answered. `--points-at` asks the ref namespace,
/// which is the registry (§2.3); more than one name for one commit (a
/// freshly forked variant) renders the first in ref order rather than
/// inventing a list — the sha beside it is the unambiguous half.
fn lineage_at(ws: &Path, commit: &str, git: &dyn GitRunner) -> Result<Option<String>, String> {
    let out = git
        .run_capture(
            &workspace::repo_git(ws),
            &[
                "for-each-ref",
                "--format=%(refname:short)",
                "--points-at",
                commit,
                "refs/heads/config/",
            ],
        )
        .map_err(|s| s.to_string())?;
    Ok(out
        .lines()
        .map(str::trim)
        .find(|l| !l.is_empty())
        .map(str::to_string))
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
