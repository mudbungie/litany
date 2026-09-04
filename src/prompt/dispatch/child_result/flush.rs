//! The checkpoint-flush seam of the §6 binding interpreter.
//!
//! A **checkpoint flush** ([`run_flush`]): a due `compaction:` clock at a
//! step boundary runs the `worker_flush` bindings. A branch with no
//! `compaction:` block is never due, so the whole seam is a no-op — the
//! general path with empty inputs.
//!
//! **Every dispatch the event binds runs, off the one compaction point**
//! (`docs/DESIGN_LEARNING_LOOP.md` §2). `worker_flush` is a list like any
//! other event's, so the learning loop's `[dispatch(compactor),
//! dispatch(reviewer)]` forks two children off the same commit: the
//! compactor, whose product lands by rebase-forward, and the reviewer,
//! whose proposal lands nowhere. One clock, one point, two forks — no
//! second trigger and no second "since last" derivation.
//!
//! **The in-flight suppressor stays keyed on the compactor** (§2.7,
//! bl-b9f0), and that is exact rather than an omission: a reviewer is
//! forked at the same boundary as its compactor sibling, so a reviewer in
//! flight implies a compaction in flight and the branch is already not
//! due. The residual is priced, not policed — a reviewer still running
//! after its sibling's landing does not suppress the next pass, which
//! costs one overlapping reviewer and is bounded by the clock the landing
//! reset.

use crate::config::{Action, Event, Workflow};
use crate::prompt::{ChildDispatchRequest, Deps, Error, child_dispatch, compactor, reviewer};
use std::path::Path;

/// Run the `worker_flush` checkpoint at a step boundary (§2.7, §6): if the
/// `compaction:` clock is due for the branch at `worktree`, run the
/// event's bound actions (default: `dispatch(compactor)`), forking each
/// bound role's child off the **compaction point** — the tip, or `HEAD~keep_recent`
/// when the workflow retains a recent tail (§2.6). A branch with no
/// `compaction:` block is never due, so this is a no-op — the general
/// path with empty inputs; so is a due clock whose whole span sits inside
/// the retained tail (nothing beneath the tail to compact).
pub(in crate::prompt::dispatch) fn run_flush(
    workspace: &Path,
    agent_id: &str,
    worktree: &Path,
    workflow: &Workflow,
    deps: &Deps<'_>,
) -> Result<(), Error> {
    // No `compaction:` clock → never due; skip the git-derived state (§2.7).
    let Some(cfg) = workflow.compaction.as_ref() else {
        return Ok(());
    };
    let state = compactor::state(worktree, agent_id, deps.clock.now_unix(), false, deps.git)?;
    if !compactor::due(Some(cfg), &state)? {
        return Ok(());
    }
    let Some(point) = compaction_point(worktree, agent_id, cfg, &state, deps.git)? else {
        return Ok(());
    };
    for action in flush_actions(workflow) {
        execute_flush(
            &action,
            workspace,
            agent_id,
            worktree,
            point.as_deref(),
            deps,
        )?;
    }
    Ok(())
}

/// The compaction point the due checkpoint forks a compactor off (§2.6):
/// `None` in the outer `Option` means the span is empty and the flush
/// skips; `None` in the inner means the tip (the fork's own default). A
/// configured `keep_recent` puts the point at `HEAD~keep_recent`, and a
/// clock whose commits-since-checkpoint have not outgrown the retained
/// tail has an empty span — the tail *is* the uncompacted content, and
/// it is retained by declaration.
///
/// `keep_recent_tokens` is the same rule in the provider's unit rather
/// than in commits (§5.2, [`compactor::checkpoint::tail`]): the point is
/// the oldest model-entry commit that leaves the stretch above it
/// costing at most `n` prompt tokens, and a branch whose whole
/// uncompacted stretch already fits has the same empty span. The two
/// keys are mutually exclusive at config load, so this reads as one
/// choice, not a precedence.
fn compaction_point(
    worktree: &Path,
    agent_id: &str,
    cfg: &crate::config::CompactionConfig,
    state: &compactor::checkpoint::CheckpointState,
    git: &dyn crate::template::GitRunner,
) -> Result<Option<Option<String>>, Error> {
    if let Some(budget) = cfg.intermediate.keep_recent_tokens {
        return Ok(compactor::checkpoint::tail::point(worktree, agent_id, budget, git)?.map(Some));
    }
    let keep = cfg.intermediate.keep_recent.unwrap_or(0);
    if keep == 0 {
        return Ok(Some(None));
    }
    if state.commits_since_checkpoint <= keep {
        return Ok(None);
    }
    let rev = format!("HEAD~{keep}");
    let sha = git
        .run_capture(worktree, &["rev-parse", &rev])
        .map_err(|source| Error::Git {
            op: "compaction point rev-parse",
            source,
        })?;
    Ok(Some(Some(sha.trim().to_string())))
}

/// The `worker_flush` actions, or the §2.7 baseline default when unbound:
/// dispatch a compactor. Overridable by binding the event.
fn flush_actions(workflow: &Workflow) -> Vec<Action> {
    let bound = workflow.actions_for(Event::WorkerFlush);
    if bound.is_empty() {
        vec![Action::Dispatch {
            role: compactor::COMPACTOR_ROLE.to_string(),
            with: None,
            mode: None,
        }]
    } else {
        bound
    }
}

/// Execute one `worker_flush` action. A dispatch of a role the harness
/// has a checkpoint goal for runs; any other closed-set action here is
/// declined (`Error::ActionUnsupported`), as is a dispatch of any other
/// role — a checkpoint fork the harness cannot instruct is a config
/// fault, and declining it loudly beats forking a child with nothing to
/// do (`docs/PRINCIPLES.md` "Decline illegal operations").
fn execute_flush(
    action: &Action,
    workspace: &Path,
    agent_id: &str,
    worktree: &Path,
    point: Option<&str>,
    deps: &Deps<'_>,
) -> Result<(), Error> {
    let unsupported = || Error::ActionUnsupported {
        action: format!("{action:?}"),
        event: Event::WorkerFlush.as_str(),
    };
    let Action::Dispatch { role, .. } = action else {
        return Err(unsupported());
    };
    // The boilerplate goal quotes the dispatching branch's own goal
    // (§2.7), read from the worktree we are forking off — `goal.md` is
    // pinned at dispatch (§2.8), so the tip's copy is the point's too.
    let goal = match role.as_str() {
        compactor::COMPACTOR_ROLE => compactor::compactor_goal(worktree, agent_id)?,
        reviewer::REVIEWER_ROLE => reviewer::reviewer_goal(worktree, agent_id)?,
        _ => return Err(unsupported()),
    };
    dispatch_at_point(role, &goal, workspace, agent_id, worktree, point, deps)
}

/// Fork one checkpoint child off `agent_id`'s compaction point — `None`
/// is the tip — and start it through the front door (§2.5, §2.7). What
/// its return does is the returning role's business on a later hop: a
/// compactor's lands the rebase-forward (`compactor_return`), a
/// reviewer's stages a proposal (`reviewer_return`).
fn dispatch_at_point(
    role: &str,
    goal: &str,
    workspace: &Path,
    agent_id: &str,
    worktree: &Path,
    point: Option<&str>,
    deps: &Deps<'_>,
) -> Result<(), Error> {
    let req = ChildDispatchRequest {
        repo: workspace,
        parent_branch: agent_id,
        parent_worktree: worktree,
        role,
        goal,
        name: None,
        fork_point: point,
        cwd: None,
        pins: crate::prompt::PinnedDocs::none(),
    };
    child_dispatch::run_procedure(
        &req,
        deps.git,
        deps.clock,
        deps.id_gen,
        deps.launcher,
        deps.rng,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::CompactionConfig;
    use crate::prompt::compactor::checkpoint::CheckpointState;
    use crate::template::GitRunner;
    use std::path::PathBuf;

    fn cfg(yaml: &str) -> CompactionConfig {
        serde_yaml_ng::from_str(yaml).unwrap()
    }
    fn state(commits: u32) -> CheckpointState {
        CheckpointState {
            commits_since_checkpoint: commits,
            seconds_since_checkpoint: 0,
            flush_requested: false,
            is_compactor: false,
            compaction_in_flight: false,
            last_usage: None,
        }
    }
    /// Scripted `rev-parse` answer; `None` fails the capture.
    struct RevGit(Option<&'static str>);
    impl GitRunner for RevGit {
        fn run(&self, _d: &Path, _a: &[&str]) -> std::io::Result<()> {
            unreachable!("compaction_point only captures")
        }
        fn run_capture(&self, _d: &Path, args: &[&str]) -> std::io::Result<String> {
            assert_eq!(args[0], "rev-parse");
            self.0
                .map(|s| format!("{s}\n"))
                .ok_or_else(|| std::io::Error::other("boom"))
        }
    }

    #[test]
    fn no_retained_tail_is_the_tip() {
        // keep_recent omitted → the point is the tip (`None` fork point) —
        // the general path, no rev-parse anywhere.
        let c = cfg("intermediate:\n  trigger: every_n_commits\n  n: 3\n");
        let p = compaction_point(&PathBuf::from("/x"), "p1", &c, &state(5), &RevGit(None)).unwrap();
        assert_eq!(p, Some(None));
    }

    #[test]
    fn a_span_inside_the_retained_tail_skips_the_flush() {
        // §2.6: commits-since-checkpoint at or under the retained tail is
        // an empty span — the uncompacted content *is* the tail, retained
        // by declaration (reachable under the time and flush triggers).
        let c = cfg("intermediate:\n  trigger: every_t_seconds\n  n: 1\n  keep_recent: 4\n");
        let p = compaction_point(&PathBuf::from("/x"), "p1", &c, &state(4), &RevGit(None)).unwrap();
        assert_eq!(p, None);
    }

    #[test]
    fn a_retained_tail_puts_the_point_behind_the_tip() {
        let c = cfg("intermediate:\n  trigger: every_n_commits\n  n: 3\n  keep_recent: 2\n");
        let p = compaction_point(
            &PathBuf::from("/x"),
            "p1",
            &c,
            &state(5),
            &RevGit(Some("abc")),
        )
        .unwrap();
        assert_eq!(p, Some(Some("abc".into())));
    }

    #[test]
    fn a_token_tail_takes_the_point_from_the_usage_walk() {
        // §5.2: under `keep_recent_tokens` the point is the token walk's
        // ([`compactor::checkpoint::tail`]), not `HEAD~keep_recent` — so
        // no `rev-parse` is issued at all, and a walk that finds the
        // whole uncompacted stretch inside the budget skips the flush the
        // same way an under-`keep_recent` span does.
        let c = cfg("intermediate:\n  trigger: on_flush\n  keep_recent_tokens: 20000\n");
        let dir = tempfile::TempDir::new().unwrap();
        // No transcript at all: the walk answers "nothing to compact"
        // before any git call, which is also what proves the token arm
        // took over from the commit arm (`RevGit` would have panicked).
        let p = compaction_point(dir.path(), "p1", &c, &state(5), &RevGit(None)).unwrap();
        assert_eq!(p, None);
    }

    #[test]
    fn a_rev_parse_failure_surfaces_as_git_error() {
        let c = cfg("intermediate:\n  trigger: every_n_commits\n  n: 3\n  keep_recent: 2\n");
        let err =
            compaction_point(&PathBuf::from("/x"), "p1", &c, &state(5), &RevGit(None)).unwrap_err();
        assert!(
            matches!(
                err,
                Error::Git {
                    op: "compaction point rev-parse",
                    ..
                }
            ),
            "{err:?}"
        );
    }
}
