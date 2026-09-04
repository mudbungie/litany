//! Per-step on-disk landings (ARCH §2.3 / §2.10).
//!
//! Step records live at `<conv-repo>/steps/<conv-id>/<NNN>/`,
//! outside every worktree (§2.2). The harness writes them as
//! diagnostic / audit artifacts and does not read them back at
//! runtime (§2.3 Diagnostic-only contract).
//!
//! Step 1's dispatch commit lays `goal.md` and `soul.md` at the
//! worktree root and commits — that single commit's tree is the
//! model-read state for step 1 (§2.10). Step ≥2 takes no pre-call
//! commit; the branch tip already represents what the model reads.
//! The `commit` field on each step's `meta.json` records that tip
//! sha so replay can re-run context assembly against the right
//! tree (§2.10) without consulting `request.json`.
//!
//! `request.json`, `response.json`, and `meta.json` land outside
//! the worktree and are not git-tracked (§2.3 — "Step records are
//! not committed to git").

mod descriptors;
pub(crate) mod inherited;
mod reviewer_read;
mod skill_bodies;
#[cfg(test)]
mod tests;
#[cfg(test)]
mod tests_facts;
mod trim;
mod unsettled;

pub(crate) use descriptors::{Grant, Undescribed, require_described};
pub(crate) use trim::trim_to_context;

use crate::prompt::Deps;
use crate::prompt::Error;
use crate::prompt::step::{META_FILE, REQUEST_FILE, StepMeta};
use serde_json::Value;
use std::path::Path;

/// Per-request `max_tokens` output cap — one model call's output
/// ceiling, distinct from the §6 spend budgets and from the §5.2
/// manifest's `budget_tokens` (an assembled-context budget, no output
/// cap). It lives beside [`write_request`], the one writer that spends
/// it, and both callers of that writer read it from here.
pub(super) const DEFAULT_MAX_TOKENS: u32 = 4096;
/// Worktree-relative path where the conversation's goal is committed
/// at dispatch time (ARCH §2.8). Lives at the worktree root so the
/// manifest's `pinned: [goal.md]` rule (§5.2) sees it.
pub(super) const GOAL_FILE: &str = "goal.md";
/// Worktree-relative path where the role's system prompt is committed
/// at dispatch time (ARCH §4.3 / §2.8). Lives at the worktree root for
/// the same reason `goal.md` does.
pub(super) const SOUL_FILE: &str = "soul.md";
/// The **system slot's files** (ARCH §2.3 *Goal and soul are pinned
/// files*, §5.2 *Structural wire homes*): the three worktree-root paths
/// whose wire home is [`compose_system`] rather than any list a
/// manifest names. This is that set's one home, beside the composer
/// that defines it (`docs/PRINCIPLES.md` single source of truth), and
/// three unrelated rules read it rather than each spelling the triple:
/// assembly refuses to compose them a second time as body text
/// ([`super::assembler`]), the pin validator reserves their names
/// ([`crate::prompt::pinned_doc`]), and the compactor's nomination gate
/// keeps them out of the compaction-eligible set (§2.7,
/// [`crate::prompt::compactor::tools`]).
pub(crate) const SYSTEM_SLOT_FILES: [&str; 3] = [
    GOAL_FILE,
    SOUL_FILE,
    crate::workspace::agent_name::NAME_FILE,
];

/// `git worktree add -b agents/<id> <worktree_path> <fork-point>`, run
/// against the workspace's bare `repo.git` (§2.2): fork the fresh root
/// agent off the ref the start named — a config lineage's head, or any
/// ref at all (§2.3 *Any ref is a legal fork point*, §7.2
/// fork-from-history). The fork chooses the lineage (§2.2, bl-403b),
/// and what `resolved` already carries is that lineage's followed
/// commit — so this call is the same operation with a different
/// argument, never a second kind of start. Root id
/// uniqueness per workspace is structural: the `-b` creation fails if
/// the ref already exists.
pub(super) fn spawn_branch(
    workspace: &Path,
    worktree_path: &Path,
    agent_id: &str,
    fork_point: &str,
    deps: &Deps<'_>,
) -> Result<(), Error> {
    let wt_str = worktree_path.to_string_lossy().to_string();
    let branch_ref = crate::workspace::agent_ref(agent_id);
    deps.git
        .run(
            &crate::workspace::repo_git(workspace),
            &[
                "worktree",
                "add",
                "-b",
                branch_ref.as_str(),
                wt_str.as_str(),
                fork_point,
            ],
        )
        .map_err(|source| Error::Git {
            op: "worktree add",
            source,
        })
}

/// Compose the system slot: the branch's goal, the agent's identity when
/// it has a name, then the role's soul. The system slot *is* the
/// pinned-head wire home for `goal.md`, `name` and `soul.md` (§2.3 "Goal
/// and soul are pinned files", §5.2 structural wire homes): assembly
/// composes all three through here, never as body text.
///
/// The goal leads, so it stays pinned at the head of every model call on
/// the branch (§2.8). The identity line is **derived here from the name
/// fact, never stored a second time** (§2.3 — the `name` file is the one
/// home; `docs/PRINCIPLES.md` single source of truth), and it states the
/// name and nothing else: no instruction rides an identity (§2.8 — the
/// name is who the agent is, not what it is to do). An unnamed agent
/// states nothing, and its slot is byte-identical to what a nameless
/// harness composed — the general path with empty inputs, not a second
/// shape.
pub(super) fn compose_system(goal: &str, name: Option<&str>, soul: &str) -> String {
    let identity = name.map_or_else(String::new, |n| format!("Your name is {n}.\n\n"));
    format!("<goal>\n{goal}\n</goal>\n\n{identity}{soul}")
}

/// Step 1: write `goal.md` + `soul.md` to the worktree root, plus any
/// caller-supplied pinned documents at their validated destinations
/// ([`crate::prompt::pinned_doc`], §2.5). Step ≥2 has no dispatch
/// artifact (the branch tip already reflects the model-read state per
/// §2.10).
pub(super) fn write_dispatch_files(
    worktree_path: &Path,
    goal_text: &str,
    soul_text: &str,
    pins: &crate::prompt::PinnedDocs,
) -> Result<(), Error> {
    std::fs::create_dir_all(worktree_path)?;
    std::fs::write(worktree_path.join(GOAL_FILE), goal_text)?;
    std::fs::write(worktree_path.join(SOUL_FILE), soul_text)?;
    pins.write_into(worktree_path)?;
    Ok(())
}

/// Step 1's dispatch commit (§2.3 step 2): remove the harness-facing
/// control files from the agent's tree (§2.2 — control is read from the
/// governing config commit; the worktree holds only context) and settle
/// the agent's `name` (§2.3), `git add goal.md soul.md`, then commit on
/// the agent branch. The removal is
/// total, not conditional: `--ignore-unmatch` makes it a no-op when the
/// fork point was not a config commit (a child forked off a parent's
/// tip, whose tree already lost them). This is the only commit the
/// harness emits for a step; §2.10 keeps step ≥2 commit-free, so the
/// branch tip after a dispatch commit *is* step 1's read state.
pub(super) fn commit_dispatch(
    worktree_path: &Path,
    conv_id: &str,
    name: Option<&str>,
    pins: &crate::prompt::PinnedDocs,
    resolved: &super::Resolved<'_>,
    deps: &Deps<'_>,
) -> Result<(), Error> {
    trim_to_context(worktree_path, conv_id, &resolved.grant, name, deps.git)?;
    let mut add_args: Vec<&str> = vec!["add", GOAL_FILE, SOUL_FILE];
    add_args.extend(pins.iter().map(crate::prompt::PinnedDoc::dest));
    deps.git
        .run(worktree_path, &add_args)
        .map_err(|source| Error::Git { op: "add", source })?;
    let msg = format!("step 001: dispatch [{conv_id}]");
    deps.git
        .run(worktree_path, &["commit", "-m", msg.as_str()])
        .map_err(|source| Error::Git {
            op: "commit",
            source,
        })
}

/// Resolve the branch tip's sha at step-start. Recorded in
/// `meta.json` so replay can re-run context assembly against the
/// right tree without reading `request.json` (§2.10 Diagnostic-only
/// contract).
pub(super) fn read_branch_tip(worktree_path: &Path, deps: &Deps<'_>) -> Result<String, Error> {
    deps.git
        .run_capture(worktree_path, &["rev-parse", "HEAD"])
        .map_err(|source| Error::Git {
            op: "rev-parse",
            source,
        })
}

/// Land `request.json` under `<conv-repo>/steps/<conv-id>/<NNN>/`.
/// Outside every worktree (§2.2) so context assembly cannot pick it
/// up; not git-tracked (§2.3).
pub(super) fn write_request(
    conv_repo: &Path,
    step_dir_rel_str: &str,
    request_value: &Value,
) -> Result<(), Error> {
    let step_dir_abs = conv_repo.join(step_dir_rel_str);
    std::fs::create_dir_all(&step_dir_abs)?;
    let bytes = serde_json::to_vec_pretty(request_value).expect("Value is always serializable");
    std::fs::write(step_dir_abs.join(REQUEST_FILE), bytes)?;
    Ok(())
}

/// Land `meta.json` under the conv-repo step dir. The `commit` field
/// is the load-bearing piece (§2.10 — replay reproduces the wire
/// input by re-running context assembly against this sha).
pub(super) fn write_meta(
    conv_repo: &Path,
    step_dir_rel_str: &str,
    meta: &StepMeta,
) -> Result<(), Error> {
    let step_dir_abs = conv_repo.join(step_dir_rel_str);
    std::fs::create_dir_all(&step_dir_abs)?;
    let bytes = serde_json::to_vec_pretty(meta).expect("StepMeta is always serializable");
    std::fs::write(step_dir_abs.join(META_FILE), bytes)?;
    Ok(())
}
