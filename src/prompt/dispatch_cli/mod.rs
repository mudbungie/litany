//! CLI handler for `litany dispatch <role>` (ARCH §3.4) — the shared id
//! guard, the fork-point guard, the front door's role-validity
//! pre-flight, the per-role `--goal` rule, and the hand-off into the
//! child-dispatch primitive.
//!
//! **`--from <ref>` is not a second kind of dispatch.** §2.3 makes any
//! ref a legal fork point and §7.2 states that forking from history "is
//! the ordinary fork with a historical ref argument … no distinct
//! operation", so the flag reaches the *existing* `fork_point` field of
//! [`ChildDispatchRequest`] — the one the §6 verifier gate already sets
//! — and changes nothing else in the dispatch: the child's id stays
//! `<parent>-<sub>` and its return address stays the dispatcher's
//! (§2.6).
//!
//! What *does* follow the fork point is the child's **governing config
//! commit**, and it must: §2.2 says an agent started by fork-back-in
//! inherits its source's config the same way, and the child's own
//! ancestry begins at that ref — so every later `litany advance`
//! resolves control from it (§6). Soul, `tools:` grant, `descriptions/**`,
//! budgets and the role-validity pre-flight below are therefore read
//! from that one commit, never from the fork point's *tree* (§3.3,
//! §5.1). A dispatch naming no fork point forks off the parent's own
//! branch, so this is the parent's config and nothing moves.
//!
//! Lives in the lib (not the bin) so the bin stays a thin shim under the
//! repo's 300-line cap and the wiring is unit-testable — the same
//! discipline as `stop::cli_run` and `inbox::cli_run`.
//!
//! **The id guard is the same rule at every verb taking an agent id from
//! outside** — `message`, `advance`, `stop`, `dispatch`, `bundle`
//! (README). `dispatch` runs it through the same two shared functions the
//! others do: [`crate::workspace::require`] for the workspace layout
//! (§2.2) and [`crate::workspace::require_agent`] for the dispatching
//! parent (§2.3), both ahead of any governing-config derivation.
//!
//! **The role set is open (§4.3).** This CLI enumerates no role names:
//! validity is the single-home config check ([`crate::prompt::role::validate`])
//! — a role is dispatchable iff the governing config commit lists it and
//! carries its soul — run *before* the fork so a rejected role leaves no
//! branch debris. Exactly one role is special-cased, and only for the
//! `--goal` rule: the compactor's goal is procedure-generated (§2.7), so
//! it is the one role that rejects `--goal`. The closed vocabulary
//! `worker`/`compactor`/`verifier` belongs to the §6 workflow
//! interpreter, never to dispatch validity (§4.3 severability line).

use super::{ChildDispatchRequest, Error};
use crate::prompt::compactor::{COMPACTOR_ROLE, compactor_goal};
use crate::prompt::inbox::{AdvanceLauncher, Launcher};
use crate::prompt::role;
use crate::prompt::{NanoIdGen, SystemClock, child_dispatch};
use crate::template::RealGit;
use crate::workspace;
use std::path::Path;

/// Role name for the compactor child (§2.7): the one role whose goal is
/// procedure-generated, so `--goal` is rejected. This names the §2.7
/// compaction procedure, not a dispatch-validity allow-list.
const ROLE_COMPACTOR: &str = COMPACTOR_ROLE;

/// Dispatch CLI failures, joined with [`Error`] under one `Display` for a
/// uniform `litany dispatch <role>:` failure line.
#[derive(Debug)]
pub enum DispatchCliError {
    /// The workspace-layout guard declined the path — the shared
    /// [`crate::workspace::require`] voice every id-taking verb uses.
    Layout(workspace::LayoutError),
    /// The dispatching parent has no `agents/*` ref — the shared
    /// [`crate::workspace::require_agent`] voice (§2.3).
    UnknownParent(workspace::UnknownAgent),
    /// `--from` named a fork point the workspace does not have — the
    /// shared [`crate::workspace::require_ref`] voice (§2.3), fired
    /// before the fork so the refusal leaves no branch behind.
    UnknownForkPoint(workspace::UnknownRef),
    /// The role is not dispatchable against the calling branch's
    /// governing config commit (not in `providers.yaml`, or its soul is
    /// missing) — the open-set membership failure (§4.3).
    InvalidRole(role::validate::Invalid),
    /// `--goal` omitted for a role that requires one (every role but the
    /// compactor).
    GoalRequired(String),
    /// `--goal` supplied for the compactor, whose goal is procedure-
    /// generated (§2.7).
    GoalForbidden(&'static str),
    Inner(Error),
}

impl std::fmt::Display for DispatchCliError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Layout(e) => write!(f, "{e}"),
            Self::UnknownParent(e) => write!(f, "{e}"),
            Self::UnknownForkPoint(e) => write!(f, "{e}"),
            Self::InvalidRole(inv) => write!(f, "{inv}"),
            Self::GoalRequired(r) => write!(f, "--goal is required for role {r:?}"),
            Self::GoalForbidden(r) => write!(f, "--goal is not accepted for role {r:?}"),
            Self::Inner(e) => write!(f, "{e}"),
        }
    }
}

impl From<Error> for DispatchCliError {
    fn from(value: Error) -> Self {
        Self::Inner(value)
    }
}

/// Run `litany dispatch <role> <repo> <branch> [--goal <text>]
/// [--name <name>] [--cwd <path>]`
/// (ARCH §3.4). Role-validity and per-role `--goal` violations surface as
/// `Err` for the bin's uniform non-zero exit. Any valid role is dispatched
/// as an ordinary child ([`child_dispatch`], §2.5); roles differ only in
/// the pinned soul (`souls/<role>.md`) and in where the goal comes from —
/// a per-dispatch `--goal` for every role but the compactor, whose goal is
/// the §2.7 boilerplate. `pins` are the caller-supplied pinned documents
/// (`--pin <dest>=<src>`, [`crate::prompt::pinned_doc`]), already
/// validated and loaded by the CLI layer; `cwd` is the seeded working
/// directory (§3.3), already resolved by the same layer.
#[allow(clippy::too_many_arguments)]
pub fn run(
    role: &str,
    repo: &Path,
    branch: &str,
    goal: Option<&str>,
    from: Option<&str>,
    name: Option<&str>,
    pins: &crate::prompt::PinnedDocs,
    cwd: Option<&Path>,
    driver_target: &Path,
) -> Result<(), DispatchCliError> {
    // The production launcher detach-spawns `litany advance` (§2.11) at
    // `driver_target` — the running-binary path the exec binding injects
    // (`cmd::Fx::driver_target`, §3.4); the library resolves none itself.
    // The launch decision is tested through [`run_with`] against an
    // injected launcher.
    let launcher = AdvanceLauncher::with_exe(driver_target.to_path_buf());
    run_with(role, repo, branch, goal, from, name, pins, cwd, &launcher)
}

/// [`run`] with the driver launcher injected — the same
/// launcher-as-parameter discipline as `inbox::probe_and_launch`, so the
/// fork + front-door deposit is exercisable without spawning a real
/// `litany advance`.
#[allow(clippy::too_many_arguments)]
fn run_with(
    role: &str,
    repo: &Path,
    parent_branch: &str,
    goal: Option<&str>,
    from: Option<&str>,
    name: Option<&str>,
    pins: &crate::prompt::PinnedDocs,
    cwd: Option<&Path>,
    launcher: &dyn Launcher,
) -> Result<(), DispatchCliError> {
    // The shared id guard, ahead of everything (§2.2, §2.3): the
    // workspace layout, then the dispatching parent's existence. It is
    // the same sequence `message`, `advance`, `stop` and `bundle` run,
    // through the same two functions — so a missing workspace or a
    // mistyped parent is declined in the product's voice here too,
    // instead of surfacing as a raw git failure from the governing-config
    // derivation below (bl-c89b).
    workspace::require(repo).map_err(DispatchCliError::Layout)?;
    workspace::require_agent(
        repo,
        parent_branch,
        "a child forks off an existing parent (ARCH §2.5)",
        &RealGit::new(),
    )
    .map_err(DispatchCliError::UnknownParent)?;

    // A named fork point is guarded by the same shared existence query
    // (§2.3 *Any ref is a legal fork point*): `--from` takes the ref
    // verbatim — a sibling's terminal ref, a historical commit, a
    // stopped tip — so an absent one is declined here, in the parent's
    // voice, rather than surfacing as a raw `git worktree add` failure
    // after the budget gate has already run.
    if let Some(rev) = from {
        workspace::require_ref(
            repo,
            rev,
            "a child forks off the ref you name (ARCH §2.3, ARCH §7.2)",
            &RealGit::new(),
        )
        .map_err(DispatchCliError::UnknownForkPoint)?;
    }

    // Open-set validity precedes the fork (§4.3): a role absent from the
    // governing config commit (unlisted, or missing its soul) is refused
    // before any branch is created, so a rejected role leaves no debris.
    // One home for the check (`role::validate`), never a name list here.
    // Asked of the config that will govern the child — the fork point's
    // when `--from` named one (§2.2 fork-back-in), the parent's
    // otherwise. Same commit the soul and the grant are read from.
    role::validate::validate(repo, parent_branch, from, role, &RealGit::new())
        .map_err(DispatchCliError::InvalidRole)?;

    // Resolve the per-role goal (§2.7): every role carries a per-dispatch
    // `--goal` except the compactor, which rejects it and uses the
    // boilerplate goal the compaction procedure owns instead.
    let goal_text = if role == ROLE_COMPACTOR {
        if goal.is_some() {
            return Err(DispatchCliError::GoalForbidden(ROLE_COMPACTOR));
        }
        // The boilerplate quotes the dispatching branch's own goal
        // (§2.7), read from that branch's worktree.
        compactor_goal(
            &crate::workspace::agent_worktree(repo, parent_branch),
            parent_branch,
        )?
    } else {
        goal.ok_or_else(|| DispatchCliError::GoalRequired(role.to_owned()))?
            .to_owned()
    };
    dispatch_child(
        repo,
        parent_branch,
        role,
        &goal_text,
        from,
        name,
        pins,
        cwd,
        launcher,
    )
}

/// Fork `role`'s child off `parent_branch` and start it through the front
/// door (§2.5), printing the child id so the `dispatch` built-in captures
/// it as the `tool_result` address (§3.3 — stdout carries one product).
/// The workspace and the parent were established by [`run_with`]'s shared
/// guard, so nothing is re-checked here.
#[allow(clippy::too_many_arguments)]
fn dispatch_child(
    repo: &Path,
    parent_branch: &str,
    role: &str,
    goal: &str,
    fork_point: Option<&str>,
    name: Option<&str>,
    pins: &crate::prompt::PinnedDocs,
    cwd: Option<&Path>,
    launcher: &dyn Launcher,
) -> Result<(), DispatchCliError> {
    let parent_worktree = crate::workspace::agent_worktree(repo, parent_branch);
    let req = ChildDispatchRequest {
        repo,
        parent_branch,
        parent_worktree: &parent_worktree,
        role,
        goal,
        name,
        fork_point,
        cwd,
        pins,
    };
    let child = child_dispatch::run(
        &req,
        &RealGit::new(),
        &SystemClock,
        &NanoIdGen,
        launcher,
        &crate::workspace::agent_name::mint::SplitMix64::from_entropy(),
    )?;
    println!("{child}");
    Ok(())
}

#[cfg(test)]
mod tests;
#[cfg(test)]
mod tests_fork_point;
