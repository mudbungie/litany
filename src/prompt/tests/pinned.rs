//! Root-path pinned documents (ARCH §2.5, [`crate::prompt::pinned_doc`]):
//! `prompt` freezes caller-supplied bytes into the step-1 dispatch
//! commit, beside `goal.md` and `soul.md` — landed at their validated
//! destinations (nested ones included) and staged by the same `git add`
//! the dispatch commit runs, so the snapshot is exact and precedes the
//! first model request. The child-path twin lives in
//! `dispatch_cli::tests` (parity, same mechanism through
//! `spawn_subagent_branch`).

use super::fixtures::*;
use crate::prompt::{PinnedDoc, PinnedDocs, run};

#[test]
fn root_pins_land_in_the_worktree_and_ride_the_dispatch_add() {
    let repo = scaffold_repo(VALID_PER_REPO_PROVIDERS_YAML, Some("system body"));
    let harness = scaffold_harness_root();
    let adapter = StubAdapter::happy(&happy_response_bytes());
    let git = StubGit::ok();
    let (clock, id) = (FixedClock::default(), FixedIdGen);
    let (sleeper, tool_executor) = (StubSleeper::default(), StubToolExecutor::ok());

    let pins = PinnedDocs::new(vec![
        PinnedDoc::new("AGENTS.md".into(), b"project law\n".to_vec()).unwrap(),
        PinnedDoc::new("docs/notes.md".into(), b"exact\x00bytes".to_vec()).unwrap(),
    ])
    .unwrap();

    run(
        repo.path(),
        "hello",
        None,
        None,
        None,
        &pins,
        None,
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

    // Exact bytes at the caller-named destinations, nested one included
    // — snapshotted before the model call (the same step-1 landing as
    // goal.md / soul.md).
    let worktree = worktree_path(repo.path());
    assert_eq!(
        std::fs::read(worktree.join("AGENTS.md")).unwrap(),
        b"project law\n"
    );
    assert_eq!(
        std::fs::read(worktree.join("docs/notes.md")).unwrap(),
        b"exact\x00bytes"
    );

    // The dispatch commit's one `git add` stages the pins with the goal
    // and soul — no second commit, no sidecar provenance record: the
    // commit itself is the inspectable home.
    let runs = git.runs.borrow();
    assert!(
        runs.iter().any(|(dest, args)| dest == &worktree
            && args == &["add", "goal.md", "soul.md", "AGENTS.md", "docs/notes.md"]),
        "dispatch add stages the pins: {runs:?}"
    );
}
