//! Disk and git error paths for [`crate::prompt::run`].
//!
//! Covers the branch-life failures inside `prompt::run`: `git
//! worktree add`, the I/O writes for the dispatch (worktree dir,
//! goal, soul) and for the diagnostic step record (request, response,
//! meta), the dispatch commit's `git add` / `git commit`, the
//! branch-tip capture (`git rev-parse`), the model-output transcript
//! entry commit. The terminal result deposit contributes no git op to a
//! user-prompted root: its reply addresses no inbox (§2.6), so the
//! branch-tip read never runs. Merge-back is gone (§2.6), so its rebase / merge / remove
//! arms are gone with it, and terminal compaction is deleted (§2.7), so
//! no compactor dispatch follows a final response. Config and adapter
//! failure paths live in [`super::errors`].

use super::fixtures::*;
use crate::prompt::Error;

/// Indexes on the StubGit's run log. The fork point is resolved first
/// (§2.3): 0 the config-lineage pool the default `--config` reads
/// (`for-each-ref`, [`crate::workspace::require_lineage`]). Next, 1, the
/// settle-the-name pre-flight's living-names scan (§2.3, the `agents/*`
/// `for-each-ref` — supplied names are checked and omitted ones minted
/// against it; the stub lists no agent refs, so no per-agent name reads
/// follow). Control resolution follows (§2.2), and it is now the
/// *ancestry* derivation
/// against that fork point — 2 the `config/*` head enumeration, 3 its
/// one `merge-base` — then the follow-the-tip derivation (§2.2,
/// bl-403b) — 4 the head-tip enumeration, 5 its one containment
/// `merge-base --is-ancestor` — then 6-10 the five `show` control reads —
/// `version` (the §10 schema-version guard, read before anything it could
/// misparse), then providers, workflow, manifest (§5.2), soul. Branch
/// work follows: 11 worktree add, 12 the dispatch commit's control-file
/// removal (§2.3 step 2), then the descriptor derivation
/// ([`DESCRIPTOR_OPS`]), then the settled-name stage ([`NAME_SETTLE_INDEX`],
/// §2.3), then dispatch add, dispatch commit, the step-1
/// drain stray-probe (`git status`, §2.11), user-message delivery add,
/// user-message delivery commit (§2.11 — the initial message is
/// delivered through the front door before step 1's read state is
/// captured), and rev-parse. Pinned as constants so the
/// transcript/terminal op-index labels stay readable.
pub(super) const WORKTREE_ADD_INDEX: usize = 11;
/// The dispatch commit's descriptor derivation (§3.3, bl-a900): one
/// `cat-file -e` per granted tool's schema (the described-grant check),
/// one per its claimed skill frontmatter, and the single `checkout` that
/// lands the lot from the governing config commit — `2·|grant| + 1` for
/// the fixtures' two-tool `worker` grant. No `rm` here: the stub
/// worktree carries no inherited `descriptions/**` to strand.
pub(super) const DESCRIPTOR_OPS: usize = 5;
/// `git add name` — the trim's fourth part, staging the agent's own name
/// fact over whatever the fork point carried (§2.3,
/// [`crate::workspace::agent_name`]). The first op after the derivation.
pub(super) const NAME_SETTLE_INDEX: usize = WORKTREE_ADD_INDEX + 2 + DESCRIPTOR_OPS;
/// `git add goal.md soul.md` for the dispatch commit.
pub(super) const DISPATCH_ADD_INDEX: usize = NAME_SETTLE_INDEX + 1;
const REV_PARSE_INDEX: usize = DISPATCH_ADD_INDEX + 5;
/// After the model call settles, the transcript writer (§2.3) commits
/// the model-output entry — `git add` then `commit` — before the loop
/// terminates (no tool_use on the happy stream).
const TRANSCRIPT_ADD_INDEX: usize = REV_PARSE_INDEX + 1;
const TRANSCRIPT_COMMIT_INDEX: usize = TRANSCRIPT_ADD_INDEX + 1;

#[test]
fn run_surfaces_worktree_create_failure() {
    // Pre-create the worktree path as a regular file so
    // `write_dispatch_files`'s create_dir_all on the worktree fails
    // on a file-not-dir component.
    let repo = scaffold_repo(VALID_PER_REPO_PROVIDERS_YAML, Some("body"));
    let wt = worktree_path(repo.path());
    std::fs::create_dir_all(wt.parent().unwrap()).unwrap();
    std::fs::write(&wt, b"blocker").unwrap();
    let adapter = StubAdapter::happy(&happy_response_bytes());
    let err = run_with_stubs(repo.path(), "hi", &adapter, &StubGit::ok()).unwrap_err();
    assert!(matches!(err, Error::Io(_)), "got {err:?}");
}

#[test]
fn run_surfaces_goal_write_failure() {
    // Worktree dir exists but goal.md is already a directory.
    let repo = scaffold_repo(VALID_PER_REPO_PROVIDERS_YAML, Some("body"));
    let wt = worktree_path(repo.path());
    std::fs::create_dir_all(wt.join("goal.md")).unwrap();
    let adapter = StubAdapter::happy(&happy_response_bytes());
    let err = run_with_stubs(repo.path(), "hi", &adapter, &StubGit::ok()).unwrap_err();
    assert!(matches!(err, Error::Io(_)), "got {err:?}");
}

#[test]
fn run_surfaces_soul_write_failure() {
    // Worktree dir + goal.md writeable, but soul.md is a directory.
    let repo = scaffold_repo(VALID_PER_REPO_PROVIDERS_YAML, Some("body"));
    let wt = worktree_path(repo.path());
    std::fs::create_dir_all(wt.join("soul.md")).unwrap();
    let adapter = StubAdapter::happy(&happy_response_bytes());
    let err = run_with_stubs(repo.path(), "hi", &adapter, &StubGit::ok()).unwrap_err();
    assert!(matches!(err, Error::Io(_)), "got {err:?}");
}

#[test]
fn run_surfaces_rev_parse_failure() {
    // Branch-tip capture for meta.json's `commit` field (§2.10) is
    // [`REV_PARSE_INDEX`]; failing it surfaces as Error::Git { op:
    // "rev-parse" }.
    let repo = scaffold_repo(VALID_PER_REPO_PROVIDERS_YAML, Some("body"));
    let adapter = StubAdapter::happy(&happy_response_bytes());
    let err = run_with_stubs(
        repo.path(),
        "hi",
        &adapter,
        &StubGit::failing_at(REV_PARSE_INDEX),
    )
    .unwrap_err();
    assert!(
        matches!(
            err,
            Error::Git {
                op: "rev-parse",
                ..
            }
        ),
        "got {err:?}"
    );
}

#[test]
fn run_surfaces_step_dir_create_failure() {
    // Step records live at the conv-repo root (§2.2). Pre-create
    // <repo>/steps as a regular file so write_request's
    // create_dir_all on <repo>/steps/<conv-id>/<NNN>/ fails on a
    // file-not-dir component.
    let repo = scaffold_repo(VALID_PER_REPO_PROVIDERS_YAML, Some("body"));
    std::fs::write(repo.path().join("steps"), b"blocker").unwrap();
    let adapter = StubAdapter::happy(&happy_response_bytes());
    let err = run_with_stubs(repo.path(), "hi", &adapter, &StubGit::ok()).unwrap_err();
    assert!(matches!(err, Error::Io(_)), "got {err:?}");
}

#[test]
fn run_surfaces_request_write_failure() {
    // Pre-create request.json under the *conv-repo's* step dir as a
    // directory so the file write fails (step records relocated out
    // of the worktree per §2.3).
    let repo = scaffold_repo(VALID_PER_REPO_PROVIDERS_YAML, Some("body"));
    let step_dir = repo.path().join("steps/ct-1-deadbeef/001");
    std::fs::create_dir_all(step_dir.join("request.json")).unwrap();
    let adapter = StubAdapter::happy(&happy_response_bytes());
    let err = run_with_stubs(repo.path(), "hi", &adapter, &StubGit::ok()).unwrap_err();
    assert!(matches!(err, Error::Io(_)), "got {err:?}");
}

#[test]
fn run_surfaces_response_write_failure() {
    let repo = scaffold_repo(VALID_PER_REPO_PROVIDERS_YAML, Some("body"));
    let step_dir = repo.path().join("steps/ct-1-deadbeef/001");
    std::fs::create_dir_all(step_dir.join("response.json")).unwrap();
    let adapter = StubAdapter::happy(&happy_response_bytes());
    let err = run_with_stubs(repo.path(), "hi", &adapter, &StubGit::ok()).unwrap_err();
    assert!(matches!(err, Error::Io(_)), "got {err:?}");
}

#[test]
fn run_surfaces_meta_write_failure() {
    let repo = scaffold_repo(VALID_PER_REPO_PROVIDERS_YAML, Some("body"));
    let step_dir = repo.path().join("steps/ct-1-deadbeef/001");
    std::fs::create_dir_all(step_dir.join("meta.json")).unwrap();
    let adapter = StubAdapter::happy(&happy_response_bytes());
    let err = run_with_stubs(repo.path(), "hi", &adapter, &StubGit::ok()).unwrap_err();
    assert!(matches!(err, Error::Io(_)), "got {err:?}");
}

/// Failing the git call at `idx` surfaces as `Error::Git { op: $op,
/// .. }`. Shared helper so each op-index test stays one line — the macro
/// path tarpaulin trips on otherwise.
fn assert_run_fails_with_git_op(idx: usize, expected_op: &'static str) {
    let repo = scaffold_repo(VALID_PER_REPO_PROVIDERS_YAML, Some("body"));
    let adapter = StubAdapter::happy(&happy_response_bytes());
    let err = run_with_stubs(repo.path(), "hi", &adapter, &StubGit::failing_at(idx)).unwrap_err();
    match err {
        Error::Git { op, .. } => assert_eq!(op, expected_op),
        other => panic!("expected Error::Git op={expected_op}, got {other:?}"),
    }
}

macro_rules! git_op_failure_test {
    ($name:ident, $idx:expr, $op:literal) => {
        #[test]
        fn $name() {
            assert_run_fails_with_git_op($idx, $op);
        }
    };
}

git_op_failure_test!(
    run_surfaces_transcript_add_failure,
    TRANSCRIPT_ADD_INDEX,
    "transcript add"
);
git_op_failure_test!(
    run_surfaces_transcript_commit_failure,
    TRANSCRIPT_COMMIT_INDEX,
    "transcript commit"
);
// No terminal git op follows: this root was prompted by the user, so its
// reply addresses no inbox and the deposit — branch-tip read included —
// never runs (§2.6, `dispatch::result_deposit::recipient`).
