//! The checkpoint-flush seam of the §6 binding interpreter.
//!
//! A **checkpoint flush** ([`run_flush`]): a due `compaction:` clock at a
//! step boundary runs `worker_flush` → `dispatch(compactor)`. A branch with
//! no `compaction:` block is never due, so the whole seam is a no-op — the
//! general path with empty inputs.

use crate::config::{Action, Event, Workflow};
use crate::prompt::{ChildDispatchRequest, Deps, Error, child_dispatch, compactor};
use std::path::Path;

/// Run the `worker_flush` checkpoint at a step boundary (§2.7, §6): if the
/// `compaction:` clock is due for the branch at `worktree`, run the
/// event's bound actions (default: `dispatch(compactor)`), forking a
/// compactor off the **compaction point** — the tip, or `HEAD~keep_recent`
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
    if !compactor::due(Some(cfg), &state) {
        return Ok(());
    }
    let Some(point) = compaction_point(worktree, cfg, &state, deps.git)? else {
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
fn compaction_point(
    worktree: &Path,
    cfg: &crate::config::CompactionConfig,
    state: &compactor::checkpoint::CheckpointState,
    git: &dyn crate::template::GitRunner,
) -> Result<Option<Option<String>>, Error> {
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

/// Execute one `worker_flush` action. The compactor dispatch is the only
/// shipped flush action; another closed-set action here is declined.
fn execute_flush(
    action: &Action,
    workspace: &Path,
    agent_id: &str,
    worktree: &Path,
    point: Option<&str>,
    deps: &Deps<'_>,
) -> Result<(), Error> {
    match action {
        Action::Dispatch { role, .. } if role == compactor::COMPACTOR_ROLE => {
            dispatch_compactor(workspace, agent_id, worktree, point, deps)
        }
        other => Err(Error::ActionUnsupported {
            action: format!("{other:?}"),
            event: Event::WorkerFlush.as_str(),
        }),
    }
}

/// Fork a compactor child off `agent_id`'s compaction point — `None` is
/// the tip — and start it through the front door (§2.5, §2.7); its
/// return lands the rebase-forward on a later hop (`compactor_return`).
fn dispatch_compactor(
    workspace: &Path,
    agent_id: &str,
    worktree: &Path,
    point: Option<&str>,
    deps: &Deps<'_>,
) -> Result<(), Error> {
    // The boilerplate goal quotes the dispatching branch's own goal
    // (§2.7), read from the worktree we are forking off — `goal.md` is
    // pinned at dispatch (§2.8), so the tip's copy is the point's too.
    let goal = compactor::compactor_goal(worktree, agent_id)?;
    let req = ChildDispatchRequest {
        repo: workspace,
        parent_branch: agent_id,
        parent_worktree: worktree,
        role: compactor::COMPACTOR_ROLE,
        goal: &goal,
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
        let p = compaction_point(&PathBuf::from("/x"), &c, &state(5), &RevGit(None)).unwrap();
        assert_eq!(p, Some(None));
    }

    #[test]
    fn a_span_inside_the_retained_tail_skips_the_flush() {
        // §2.6: commits-since-checkpoint at or under the retained tail is
        // an empty span — the uncompacted content *is* the tail, retained
        // by declaration (reachable under the time and flush triggers).
        let c = cfg("intermediate:\n  trigger: every_t_seconds\n  n: 1\n  keep_recent: 4\n");
        let p = compaction_point(&PathBuf::from("/x"), &c, &state(4), &RevGit(None)).unwrap();
        assert_eq!(p, None);
    }

    #[test]
    fn a_retained_tail_puts_the_point_behind_the_tip() {
        let c = cfg("intermediate:\n  trigger: every_n_commits\n  n: 3\n  keep_recent: 2\n");
        let p =
            compaction_point(&PathBuf::from("/x"), &c, &state(5), &RevGit(Some("abc"))).unwrap();
        assert_eq!(p, Some(Some("abc".into())));
    }

    #[test]
    fn a_rev_parse_failure_surfaces_as_git_error() {
        let c = cfg("intermediate:\n  trigger: every_n_commits\n  n: 3\n  keep_recent: 2\n");
        let err = compaction_point(&PathBuf::from("/x"), &c, &state(5), &RevGit(None)).unwrap_err();
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
