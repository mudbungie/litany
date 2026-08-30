//! Shared subagent-dispatch primitive (ARCH §2.3 step 2, §2.5).
//!
//! Every dispatched subagent — compactor (§2.7), worker (§2.5) — starts
//! with the same on-disk shape: a branch off the parent's tip, a sibling
//! worktree at `<conv-repo>/<full-descent>/` (§2.2), `goal.md` (and, for
//! roles with a per-dispatch soul, `soul.md`) at the worktree root, the
//! agent's `name` settled by the trim (§2.3), all
//! committed as the dispatch commit. ARCH §2.5 calls dispatch "the
//! primitive"; this module is its in-process realization, shared between
//! the role-specific entry points.
//!
//! The function is `pub(crate)` because the only legitimate callers are
//! sibling modules within `prompt::` — the CLI surface for
//! procedure-to-procedure invocation is `litany dispatch <role>` per
//! §3.4, never a direct library call.

use super::Error;
use crate::template::GitRunner;
use std::path::Path;

/// Worktree-relative goal artifact (ARCH §2.8). At the worktree root
/// so manifest pinning (§5.2) sees it.
pub(crate) const GOAL_FILE: &str = "goal.md";
/// Worktree-relative soul artifact (ARCH §2.3 step 2 / §4.3). At the
/// worktree root for the same reason `goal.md` is.
pub(crate) const SOUL_FILE: &str = "soul.md";

/// Inputs to a subagent dispatch's spawn step. Held as a struct so the
/// compactor and worker call sites pass identically-shaped requests.
pub(crate) struct SpawnRequest<'a> {
    /// Parent worktree — the dispatching branch's working tree. Owns
    /// the `.git` dir's view of `parent_branch`'s tip; this is where
    /// `git worktree add` runs (ARCH §2.2 — the conv-repo root itself
    /// is not a checkout in v0.3).
    pub(crate) parent_worktree: &'a Path,
    /// New subagent branch name. By convention `<parent>-<sub-id>`,
    /// where `<sub-id>` is `<ts>-<short-id>` (§2.2 hyphenated descent).
    pub(crate) sub_branch: &'a str,
    /// Sibling worktree where the new branch is checked out. Same
    /// name as `sub_branch` so on-disk layout and ref namespace are
    /// isomorphic (§2.2).
    pub(crate) sub_worktree: &'a Path,
    /// The ref the new branch forks off (ARCH §2.3 *Any ref is a legal
    /// fork point*) — the dispatching branch for an ordinary child
    /// dispatch (§2.5), any other ref when the dispatch named one: a
    /// **verifier** forks off the *worker's terminal ref* (§6 gate) so it
    /// inherits the work it must judge, `litany dispatch --from` off
    /// whatever the caller named (§7.2). Either way the new branch is
    /// still named a child of its dispatcher, so its id — and so its
    /// return address — stays `<parent>-<sub>` (§2.6).
    ///
    /// Not an `Option`: the caller resolves the default, because it must
    /// derive the child's governing config commit from this same ref
    /// (§2.2, `child_dispatch::run`) and a second home for "which ref"
    /// is exactly how the branch and its config come to disagree.
    pub(crate) fork_point: &'a str,
    /// Goal text written to `<sub_worktree>/goal.md` and committed.
    pub(crate) goal_text: &'a str,
    /// The child's **name** (ARCH §2.3, §2.11) — its display fact,
    /// settled onto `<sub_worktree>/name` by the dispatch commit's trim
    /// ([`crate::prompt::dispatch::trim_to_context`]). `None` for an
    /// unnamed child, which is every harness-initiated dispatch: a
    /// compactor and a verifier are procedure children, not agents an
    /// operator speaks to by name.
    pub(crate) name: Option<&'a str>,
    /// Soul text written to `<sub_worktree>/soul.md` when supplied.
    /// `None` for roles whose dispatch has no per-dispatch soul (e.g. the
    /// v0.3 compactor stub: no model call, so no soul to compose).
    pub(crate) soul_text: Option<&'a str>,
    /// Caller-supplied pinned documents (§2.5,
    /// [`crate::prompt::pinned_doc`]), written at their validated
    /// destinations and committed on the dispatch commit beside
    /// `goal.md`. Harness-initiated dispatches pass
    /// [`crate::prompt::PinnedDocs::none`].
    pub(crate) pins: &'a crate::prompt::PinnedDocs,
    /// The child role, its `tools:` grant and the governing config
    /// commit both were read from (ARCH §4.3, §2.2). The dispatch commit
    /// derives the child's `descriptions/**` from that commit, filtered
    /// to the grant, so the child's tree documents exactly what its wire
    /// array will declare — and is never capped by what the parent's own
    /// grant left in the tree it forks off (§3.3, §5.1).
    pub(crate) grant: &'a crate::prompt::dispatch::Grant<'a>,
    /// Commit message subject for the dispatch commit. Each role
    /// keeps its own phrasing so `git log --oneline` legibly
    /// distinguishes the role at a glance — compactor uses
    /// `compaction: dispatch [...]`; worker uses `dispatch: worker [...]`.
    pub(crate) commit_subject: &'a str,
}

/// Spawn the subagent branch and write the dispatch commit. Steps,
/// in order:
///
/// 1. `git worktree add -b agents/<sub-id> <sub_worktree>
///    agents/<parent-id>` in the parent worktree (any access point onto
///    the one workspace repository, §2.2). Ids are bare hyphenated
///    descents; the `agents/` ref prefix is applied here, at the git
///    boundary (§2.3).
/// 2. Trim the forked tree to the child's context (§2.2, §2.3 step 2,
///    §5.1): the config commit's control files leave, and the
///    `descriptions/**` descriptors are derived from the governing
///    config commit to the child's own grant — checked out from it, not
///    inherited from the fork point (`--ignore-unmatch` keeps the
///    removal half total whatever that point carried). Then the fork
///    point's **inherited dialog** leaves (`messages/**`, `summary/**`,
///    `skills/**` — branch-scoped, §2.2): a child's opening context is
///    its goal, soul and pins plus what is deposited to it, never its
///    dispatcher's conversation — except the compactor, whose subject
///    that conversation is
///    ([`crate::prompt::dispatch::prune_inherited_dialog`]). This part
///    is the child spawn's own, not [`trim_to_context`]'s: a
///    fork-back-in *root* resumes the conversation it forks (§7.2) and
///    must keep it.
/// 3. Write `goal.md` (and `soul.md` when supplied) to the new worktree,
///    plus any caller-supplied pinned documents at their validated
///    destinations (§2.5, [`crate::prompt::pinned_doc`]).
/// 4. `git add` the artifacts.
/// 5. `git commit -m <commit_subject>` — the dispatch commit (§2.3
///    step 2 / §2.10). Step 1 of the subagent's own step loop, when
///    one runs, takes no further pre-call commit; the dispatch commit
///    *is* its read state.
pub(crate) fn spawn_subagent_branch(
    req: &SpawnRequest<'_>,
    git: &dyn GitRunner,
) -> Result<(), Error> {
    let wt_str = req.sub_worktree.to_string_lossy().to_string();
    let sub_ref = crate::workspace::agent_ref(req.sub_branch);
    git.run(
        req.parent_worktree,
        &[
            "worktree",
            "add",
            "-b",
            sub_ref.as_str(),
            wt_str.as_str(),
            req.fork_point,
        ],
    )
    .map_err(|source| Error::Git {
        op: "worktree add",
        source,
    })?;

    // `git worktree add` creates the directory in production; the
    // explicit `create_dir_all` is here for stub-git tests (and is a
    // harmless no-op in production since the directory already exists).
    std::fs::create_dir_all(req.sub_worktree)?;
    crate::prompt::dispatch::trim_to_context(req.sub_worktree, req.grant, req.name, git)?;
    crate::prompt::dispatch::prune_inherited_dialog(req.sub_worktree, req.grant.role, git)?;
    std::fs::write(req.sub_worktree.join(GOAL_FILE), req.goal_text)?;
    if let Some(soul) = req.soul_text {
        std::fs::write(req.sub_worktree.join(SOUL_FILE), soul)?;
    }
    req.pins.write_into(req.sub_worktree)?;

    let mut add_args: Vec<&str> = vec!["add", GOAL_FILE];
    if req.soul_text.is_some() {
        add_args.push(SOUL_FILE);
    }
    add_args.extend(req.pins.iter().map(crate::prompt::PinnedDoc::dest));
    git.run(req.sub_worktree, &add_args)
        .map_err(|source| Error::Git { op: "add", source })?;

    git.run(req.sub_worktree, &["commit", "-m", req.commit_subject])
        .map_err(|source| Error::Git {
            op: "commit",
            source,
        })?;

    Ok(())
}

#[cfg(test)]
mod tests;
