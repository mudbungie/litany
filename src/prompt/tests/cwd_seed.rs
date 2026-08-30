//! The seeded working directory at a root start (ARCH §3.3,
//! `litany prompt --cwd`) and the act's one home, [`seed_cwd`]. The
//! child-path twin lives in `child_dispatch::tests::cwd` (parity, same
//! function); the mark's own storage contract is covered against real
//! git in `workspace::cwd::tests`.

use super::fixtures::*;
use crate::prompt::{run, seed_cwd};
use crate::template::RealGit;
use crate::workspace::{cwd, fixture};

#[test]
fn a_seed_writes_the_named_directory_as_the_new_agents_mark() {
    let (_h, ws) = fixture::workspace();
    fixture::spawn_root(&ws, "a");
    let git = RealGit::new();
    seed_cwd(&ws, "a", Some(std::path::Path::new("/tmp")), &git).unwrap();
    assert_eq!(cwd::read(&ws, "a", &git), Some("/tmp".into()));
}

#[test]
fn no_seed_leaves_the_mark_unset_which_is_the_worktree() {
    // The general path with the fact absent — not a bootstrap case.
    let (_h, ws) = fixture::workspace();
    fixture::spawn_root(&ws, "a");
    let git = RealGit::new();
    seed_cwd(&ws, "a", None, &git).unwrap();
    assert_eq!(cwd::read(&ws, "a", &git), None);
}

#[test]
fn a_mark_that_will_not_write_fails_the_creation() {
    // No agent starts in a directory its caller did not ask for: a git
    // that cannot store the mark aborts rather than falling back to the
    // worktree.
    let holder = tempfile::TempDir::new().unwrap();
    let err = seed_cwd(
        holder.path(),
        "a",
        Some(std::path::Path::new("/tmp")),
        &RealGit::new(),
    )
    .unwrap_err();
    assert!(err.to_string().contains("seed working directory"), "{err}");
}

#[test]
fn a_root_start_seeds_the_mark_at_its_own_id_before_the_branch_exists() {
    let repo = scaffold_repo(VALID_PER_REPO_PROVIDERS_YAML, Some("system body"));
    let harness = scaffold_harness_root();
    let adapter = StubAdapter::happy(&happy_response_bytes());
    let git = StubGit::ok();
    let (clock, id) = (FixedClock::default(), FixedIdGen);
    let (sleeper, tool_executor) = (StubSleeper::default(), StubToolExecutor::ok());

    let branch = run(
        repo.path(),
        "hello",
        None,
        None,
        None,
        crate::prompt::PinnedDocs::none(),
        Some(std::path::Path::new("/tmp")),
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

    // The mark is the agent's own (§3.3 — keyed by agent id), and it is
    // written before the branch: `worktree add` comes after it.
    let runs = git.runs.borrow();
    let mark = format!("refs/litany/cwd/{branch}");
    let seeded = runs
        .iter()
        .position(|(_, args)| args[0] == "update-ref" && args.get(1) == Some(&mark))
        .unwrap_or_else(|| panic!("the start seeded its own mark: {runs:?}"));
    let forked = runs
        .iter()
        .position(|(_, args)| args[0] == "worktree")
        .expect("the start forked a branch");
    assert!(seeded < forked, "seed precedes the fork: {runs:?}");
}
