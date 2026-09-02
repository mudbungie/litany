//! Budget enforcement at the model-call boundary (ARCH §6, v0.7): a
//! conversation that exhausts `max_total_tokens` is stopped before the
//! next adapter invocation and gets the
//! `refs/litany/budget-exhausted/<branch>` marker; an unbounded workflow
//! never triggers a stop.

use super::fixtures::*;
use crate::prompt::run;
use brazen::FinishReason;
use serde_json::json;

/// `budgets: {max_total_tokens: 8}` — exactly one `stream_of` step's
/// `Usage{input:5, output:3}` = 8 tokens, so the step-2 boundary check
/// trips at `8 >= 8`.
const WORKFLOW_WITH_TOKEN_BUDGET: &str = "events: {}\nbudgets:\n  max_total_tokens: 8\n";

fn tool_use_stream() -> Vec<u8> {
    // Finishes `tool_use` so the loop advances toward a second step.
    stream_of(
        FinishReason::ToolUse,
        &[Block::ToolUse {
            id: "toolu_01",
            name: "bash",
            input: json!({ "cmd": "ls" }),
        }],
    )
}

#[test]
fn exhausted_conversation_stops_before_next_model_call_and_marks_the_ref() {
    let repo = scaffold_repo_with_workflow(
        VALID_PER_REPO_PROVIDERS_YAML,
        WORKFLOW_WITH_TOKEN_BUDGET,
        Some("body"),
    );
    let harness = scaffold_harness_root();
    // Three adapter replies are scripted: the step-1 version guard, step
    // 1's model call, and the step-2 boundary's own version guard — its
    // resolution precedes the budget check, because the ceiling being
    // checked is the freshly followed one (bl-e580). Had the check failed
    // to stop the loop, step 2's model call would invoke the adapter a
    // fourth time and the stub would panic — so "no model call" is
    // enforced structurally as well as asserted below.
    let adapter = StubAdapter::scripted([
        StubAdapter::reply_ok(&version_line()),
        StubAdapter::reply_ok(&tool_use_stream()),
        StubAdapter::reply_ok(&version_line()),
    ]);
    let git = StubGit::ok();
    let (clock, id) = (FixedClock::default(), FixedIdGen);
    let (sleeper, tool_executor) = (StubSleeper::default(), StubToolExecutor::ok());

    let branch = run(
        repo.path(),
        "go",
        None,
        None,
        None,
        crate::prompt::PinnedDocs::none(),
        None,
        &valid_deps(
            &adapter,
            &sleeper,
            &git,
            &clock,
            &id,
            &tool_executor,
            harness.path(),
        ),
    )
    .unwrap();
    assert_eq!(branch, "ct-1-deadbeef");

    // Step 1 landed 8 tokens; the step-2 boundary check tripped (8 >= 8),
    // so the adapter was invoked exactly three times: the two version
    // guards and step 1's model call — never a step-2 model call.
    assert_eq!(adapter.observed.borrow().len(), 3);
    assert!(repo.path().join("steps/ct-1-deadbeef/001").exists());
    // Step 2 was abandoned before its model call — no step-2 record.
    assert!(!repo.path().join("steps/ct-1-deadbeef/002").exists());

    // The git-native marker was written at the branch tip (§6).
    let runs = git.runs.borrow();
    assert!(
        runs.iter().any(|(_, args)| args
            == &vec![
                "update-ref".to_string(),
                "refs/litany/budget-exhausted/ct-1-deadbeef".to_string(),
                "HEAD".to_string(),
            ]),
        "expected budget-exhausted update-ref; got {runs:?}"
    );
    // Terminal-by-exhaustion: no compaction dispatch, no rebase/merge.
    assert!(
        !runs.iter().any(|(_, args)| {
            let head = args.first().map(String::as_str);
            head == Some("rebase") || head == Some("merge")
        }),
        "exhausted conversation must not merge back; got {runs:?}"
    );
}

#[test]
fn unbounded_workflow_never_triggers_a_budget_stop() {
    // No `budgets:` block → every limit unbounded → the loop runs to a
    // normal terminal completion with no budget stop (baseline).
    let repo = scaffold_repo(VALID_PER_REPO_PROVIDERS_YAML, Some("body"));
    let harness = scaffold_harness_root();
    let adapter = StubAdapter::happy(&happy_response_bytes());
    let git = StubGit::ok();
    let (clock, id) = (FixedClock::default(), FixedIdGen);
    let (sleeper, tool_executor) = (StubSleeper::default(), StubToolExecutor::ok());

    run(
        repo.path(),
        "go",
        None,
        None,
        None,
        crate::prompt::PinnedDocs::none(),
        None,
        &valid_deps(
            &adapter,
            &sleeper,
            &git,
            &clock,
            &id,
            &tool_executor,
            harness.path(),
        ),
    )
    .unwrap();

    // Ran to a normal terminal completion — no budget stop, no ref.
    let runs = git.runs.borrow();
    assert!(
        !runs.iter().any(|(_, args)| {
            args.iter()
                .any(|a| a.starts_with("refs/litany/budget-exhausted/"))
        }),
        "no budget-exhausted ref expected under an unbounded workflow"
    );
}

#[test]
fn budget_ref_write_failure_surfaces_as_a_git_error() {
    // Indices below are relative to the first post-control op; the
    // start's preamble precedes them with ten: the fork-point
    // lineage query (§2.3), the `config/*` head enumeration and its
    // merge-base (the governing ancestry derivation, §2.2), the
    // followed-tip enumeration and its containment merge-base (§2.2,
    // bl-403b), and
    // five `show` reads (`version` first, the §10 schema-version guard;
    // manifest.yaml last before the soul, §5.2).
    // The marker `update-ref` is git op #32 in the exhaustion path (0
    // worktree add, 1 control rm, 2-6 the descriptor derivation (§3.3 —
    // four `cat-file -e` existence reads against the governing config
    // commit and one `checkout`), 7 dispatch add, 8 dispatch commit, 9
    // step-1 drain stray-probe, 10/11 user-message delivery add+commit,
    // 12 step-1 rev-parse, 13/14 step-1 model-output transcript
    // add+commit, 15 the tool window's hold-mark probe (§3.3 *Tool
    // control*), 16/17 the tool transcript add+commit, 18-29 the step-2
    // boundary's own config resolution (bl-e580 — the ceiling checked
    // below is the freshly followed one, so it is read before the
    // check), 30 step-2 drain stray-probe, 31 step-2 rev-parse, 32
    // mark_exhausted update-ref).
    // Failing it surfaces the §6 exhaustion write's error arm.
    let repo = scaffold_repo_with_workflow(
        VALID_PER_REPO_PROVIDERS_YAML,
        WORKFLOW_WITH_TOKEN_BUDGET,
        Some("body"),
    );
    let harness = scaffold_harness_root();
    let adapter = StubAdapter::scripted([
        StubAdapter::reply_ok(&version_line()),
        StubAdapter::reply_ok(&tool_use_stream()),
        StubAdapter::reply_ok(&version_line()),
    ]);
    let git = StubGit::failing_at(44);
    let (clock, id) = (FixedClock::default(), FixedIdGen);
    let (sleeper, tool_executor) = (StubSleeper::default(), StubToolExecutor::ok());

    let err = run(
        repo.path(),
        "go",
        None,
        None,
        None,
        crate::prompt::PinnedDocs::none(),
        None,
        &valid_deps(
            &adapter,
            &sleeper,
            &git,
            &clock,
            &id,
            &tool_executor,
            harness.path(),
        ),
    )
    .unwrap_err();
    assert!(
        matches!(
            err,
            crate::prompt::Error::Git {
                op: "budget-exhausted update-ref",
                ..
            }
        ),
        "got {err:?}"
    );
}
