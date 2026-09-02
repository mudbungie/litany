//! Git failure paths for the branch-spawn half of [`crate::prompt::run`]
//! (ARCH §2.2–§2.3): the governing-config read, `git worktree add`, the
//! dispatch commit's control-file removal, and the dispatch add/commit.
//! Split from [`super::errors_disk`] for the per-file line cap; the
//! op-index constants live there.

use super::errors_disk::{DISPATCH_ADD_INDEX, NAME_SETTLE_INDEX, WORKTREE_ADD_INDEX};
use super::fixtures::*;
use crate::prompt::Error;

#[test]
fn run_surfaces_a_fork_point_query_failure() {
    // The very first git op resolves the fork point (§2.3): the
    // config-lineage pool the default start reads. Its failure is the
    // fork point's, ahead of any control read.
    let repo = scaffold_repo(VALID_PER_REPO_PROVIDERS_YAML, Some("body"));
    let adapter = StubAdapter::happy(&happy_response_bytes());
    let err = run_with_stubs(repo.path(), "hi", &adapter, &StubGit::failing_at(0)).unwrap_err();
    assert!(matches!(err, Error::ForkPoint(_)), "got {err:?}");
}

#[test]
fn run_surfaces_name_scan_failure() {
    // Op 1 is the settle-the-name pre-flight's living-names scan (§2.3):
    // supplied or minted, the name settles against it, and its failure is
    // the name's own refusal voice — nothing has forked.
    let repo = scaffold_repo(VALID_PER_REPO_PROVIDERS_YAML, Some("body"));
    let adapter = StubAdapter::happy(&happy_response_bytes());
    let err = run_with_stubs(repo.path(), "hi", &adapter, &StubGit::failing_at(1)).unwrap_err();
    assert!(matches!(err, Error::NameUnavailable(_)), "got {err:?}");
}

#[test]
fn run_surfaces_followed_config_read_failure() {
    // With the fork point resolved and the name settled, the next ops
    // derive the followed config commit — the governing ancestry query
    // and the tip walk over it (§2.2, bl-403b); failing any of them
    // surfaces as the followed-config error.
    let repo = scaffold_repo(VALID_PER_REPO_PROVIDERS_YAML, Some("body"));
    let adapter = StubAdapter::happy(&happy_response_bytes());
    let err = run_with_stubs(repo.path(), "hi", &adapter, &StubGit::failing_at(2)).unwrap_err();
    assert!(
        matches!(
            err,
            Error::Git {
                op: "followed config",
                ..
            }
        ),
        "got {err:?}"
    );
}

#[test]
fn run_surfaces_control_rm_failure() {
    // The dispatch commit's control-file removal (§2.3 step 2) fails.
    let repo = scaffold_repo(VALID_PER_REPO_PROVIDERS_YAML, Some("body"));
    let adapter = StubAdapter::happy(&happy_response_bytes());
    let err = run_with_stubs(
        repo.path(),
        "hi",
        &adapter,
        &StubGit::failing_at(WORKTREE_ADD_INDEX + 1),
    )
    .unwrap_err();
    assert!(
        matches!(
            err,
            Error::Git {
                op: "rm control files",
                ..
            }
        ),
        "got {err:?}"
    );
}

#[test]
fn run_surfaces_worktree_add_failure() {
    // version guard passes; `git worktree add` fails.
    let repo = scaffold_repo(VALID_PER_REPO_PROVIDERS_YAML, Some("body"));
    let adapter = StubAdapter::happy(&happy_response_bytes());
    let err = run_with_stubs(
        repo.path(),
        "hi",
        &adapter,
        &StubGit::failing_at(WORKTREE_ADD_INDEX),
    )
    .unwrap_err();
    assert!(
        matches!(
            err,
            Error::Git {
                op: "worktree add",
                ..
            }
        ),
        "got {err:?}"
    );
}

#[test]
fn run_surfaces_dispatch_add_failure() {
    // git add for the dispatch commit fails.
    let repo = scaffold_repo(VALID_PER_REPO_PROVIDERS_YAML, Some("body"));
    let adapter = StubAdapter::happy(&happy_response_bytes());
    let err = run_with_stubs(
        repo.path(),
        "hi",
        &adapter,
        &StubGit::failing_at(DISPATCH_ADD_INDEX),
    )
    .unwrap_err();
    assert!(matches!(err, Error::Git { op: "add", .. }));
}

#[test]
fn run_surfaces_dispatch_commit_failure() {
    let repo = scaffold_repo(VALID_PER_REPO_PROVIDERS_YAML, Some("body"));
    let adapter = StubAdapter::happy(&happy_response_bytes());
    let err = run_with_stubs(
        repo.path(),
        "hi",
        &adapter,
        &StubGit::failing_at(DISPATCH_ADD_INDEX + 1),
    )
    .unwrap_err();
    assert!(matches!(err, Error::Git { op: "commit", .. }));
}

#[test]
fn run_surfaces_name_settle_failure() {
    // The trim's fourth part on the root path (§2.3): staging the
    // settled `name` fails, reported in the dispatch commit's own voice.
    let repo = scaffold_repo(VALID_PER_REPO_PROVIDERS_YAML, Some("body"));
    let adapter = StubAdapter::happy(&happy_response_bytes());
    let err = run_with_stubs(
        repo.path(),
        "hi",
        &adapter,
        &StubGit::failing_at(NAME_SETTLE_INDEX),
    )
    .unwrap_err();
    assert!(matches!(
        err,
        Error::Git {
            op: "settle the agent name",
            ..
        }
    ));
}
