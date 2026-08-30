//! The §6 depth boundary at the model-call boundary ("The depth
//! boundary"): `max_depth` is the deepest *allowed* depth, so exhaustion
//! is strict — `depth(agent) > max_depth`. These two tests are the
//! off-by-one, driven through the real child hop (`litany advance`) with
//! one fixed agent and the ceiling moved around it: at exactly
//! `max_depth` the agent makes its model call; one deeper it makes none.
//! `budget_enforcement.rs` covers the same boundary on the token axis;
//! `budget/tests/enforce.rs` covers the predicate itself.

use super::fixtures::*;
use super::parent_revival::{CHILD, child_with_mail};
use crate::config::Workflow;
use crate::prompt::dispatch::advance::{AdvanceOutcome, run};
use crate::prompt::resolve::WorkerConfig;
use std::path::Path;

/// `CHILD` is `<a>-<b>` — one dispatch below its root, so depth 1 (§6:
/// depth counts dispatches from the root agent at depth 0).
const CHILD_DEPTH: u32 = 1;

/// A `WorkerConfig` whose only declared budget is `max_depth: n`.
fn capped(n: u32) -> WorkerConfig {
    WorkerConfig {
        workflow: Workflow::parse(
            &format!("events: {{}}\nbudgets:\n  max_depth: {n}\n"),
            Path::new("workflow.yaml"),
        )
        .unwrap(),
        ..super::advance::worker_config()
    }
}

#[test]
fn depth_equal_to_max_depth_is_allowed_and_the_agent_makes_its_model_call() {
    assert_eq!(crate::prompt::budget::derive::depth(CHILD), CHILD_DEPTH);
    let ws = child_with_mail();
    let adapter = StubAdapter::scripted([StubAdapter::reply_ok(&happy_response_bytes())]);
    let (sleeper, git) = (StubSleeper::default(), StubGit::ok());
    let (clock, id, tools) = (FixedClock::default(), FixedIdGen, StubToolExecutor::ok());
    let deps = valid_deps(&adapter, &sleeper, &git, &clock, &id, &tools, ws.path());

    let out = run(ws.path(), CHILD, None, &deps, &mut || {
        Ok(capped(CHILD_DEPTH))
    })
    .unwrap();

    assert!(matches!(out, AdvanceOutcome::Terminal), "{out:?}");
    assert_eq!(
        adapter.observed.borrow().len(),
        1,
        "an agent at exactly max_depth is not depth-exhausted"
    );
    assert!(
        ws.path()
            .join(format!("steps/{CHILD}/001/response.json"))
            .exists(),
        "the step record of the model call it was allowed to make"
    );
    assert!(
        !git.runs.borrow().iter().any(|(_, args)| {
            args.iter()
                .any(|a| a.starts_with("refs/litany/budget-exhausted/"))
        }),
        "no exhaustion marker at the ceiling"
    );
}

#[test]
fn depth_one_over_max_depth_exhausts_before_any_model_call() {
    let ws = child_with_mail();
    let adapter = unreachable_adapter();
    let (sleeper, git) = (StubSleeper::default(), StubGit::ok());
    let (clock, id, tools) = (FixedClock::default(), FixedIdGen, StubToolExecutor::ok());
    let deps = valid_deps(&adapter, &sleeper, &git, &clock, &id, &tools, ws.path());

    // One below the ceiling: depth 1 against `max_depth: 0`. The adapter
    // panics if invoked, so "no model call" is structural, not asserted.
    let out = run(ws.path(), CHILD, None, &deps, &mut || {
        Ok(capped(CHILD_DEPTH - 1))
    })
    .unwrap();

    assert!(matches!(out, AdvanceOutcome::Terminal), "{out:?}");
    assert!(
        !ws.path().join(format!("steps/{CHILD}")).exists(),
        "exhausted at its first boundary — no step was ever recorded"
    );
    assert!(
        git.runs.borrow().iter().any(|(_, args)| args
            == &vec![
                "update-ref".to_string(),
                format!("refs/litany/budget-exhausted/{CHILD}"),
                "HEAD".to_string(),
            ]),
        "the git-native exhaustion marker (§6); got {:?}",
        git.runs.borrow()
    );
}
