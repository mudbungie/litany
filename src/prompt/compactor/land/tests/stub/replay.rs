//! The **replay** half of the scripted landing arms (`super`): every
//! rebase-forward outcome the stub can drive — a non-conflict rebase
//! failure, the live-branch-wins resolution, the both-sides decline and
//! its two own failure modes. Split from [`super`] to hold the per-file
//! line cap; the [`Script`] they run on is its.

use super::*;

#[test]
fn a_rebase_failure_with_no_conflict_aborts_and_surfaces() {
    // A rebase that failed for a non-conflict reason (dirty tree, bad
    // ref): nothing unmerged, so the landing aborts and surfaces the
    // rebase's own failure.
    let s = Script {
        rebase_fails: RefCell::new(1),
        ..Script::ok()
    };
    assert_op(s.land().unwrap_err(), "rebase-forward rebase");
}

#[test]
fn an_unmerged_listing_failure_surfaces() {
    let s = Script {
        rebase_fails: RefCell::new(1),
        fail_capture: Some("ls-files"),
        ..Script::ok()
    };
    assert_op(s.land().unwrap_err(), "rebase-forward unmerged");
}

// Stages 1+3 on `code.txt`, plus two lines the parser must skip over:
// one with no tab separator, one with a stage token outside 1/2/3.
const MODIFY_DELETE: &str =
    "100644 aaa 1\tcode.txt\nnot a stage line\n100644 eee 9\tcode.txt\n100644 ccc 3\tcode.txt\n";
const BOTH_SIDES: &str = "100644 bbb 2\tsummary/001.md\n100644 ccc 3\tsummary/001.md\n";

#[test]
fn a_live_branch_wins_stop_resolves_and_continues() {
    let s = Script {
        rebase_fails: RefCell::new(1),
        ls_files: MODIFY_DELETE,
        ..Script::ok()
    };
    assert_eq!(s.land().unwrap(), LandOutcome::Landed);
}

#[test]
fn a_live_branch_wins_add_failure_surfaces() {
    // `"add -- "` matches the replay's `add -- <path>` and never the
    // base build's `worktree add --no-checkout`.
    let s = Script {
        rebase_fails: RefCell::new(1),
        ls_files: MODIFY_DELETE,
        fail_run: Some("add -- "),
        ..Script::ok()
    };
    assert_op(s.land().unwrap_err(), "rebase-forward live-branch-wins add");
}

#[test]
fn more_stops_than_commits_aborts_rather_than_spins() {
    // One commit to replay (`--count` → 1) but endless conflict stops:
    // git is not making progress, so the landing aborts loudly.
    let s = Script {
        rebase_fails: RefCell::new(99),
        ls_files: MODIFY_DELETE,
        ..Script::ok()
    };
    assert_op(s.land().unwrap_err(), "rebase-forward rebase");
}

#[test]
fn a_both_sides_conflict_declines_with_the_paths() {
    let s = Script {
        rebase_fails: RefCell::new(1),
        ls_files: BOTH_SIDES,
        ..Script::ok()
    };
    assert_eq!(
        s.land().unwrap(),
        LandOutcome::Conflicted(vec!["summary/001.md".to_string()])
    );
}

#[test]
fn a_decline_abort_failure_surfaces() {
    let s = Script {
        rebase_fails: RefCell::new(1),
        ls_files: BOTH_SIDES,
        fail_run: Some("--abort"),
        ..Script::ok()
    };
    assert_op(s.land().unwrap_err(), "rebase-forward abort");
}

#[test]
fn a_decline_mark_failure_surfaces() {
    let s = Script {
        rebase_fails: RefCell::new(1),
        ls_files: BOTH_SIDES,
        fail_run: Some("update-ref"),
        ..Script::ok()
    };
    assert_op(s.land().unwrap_err(), "rebase-forward decline update-ref");
}

#[test]
fn a_missing_dispatch_commit_is_declined() {
    let s = Script {
        log: "",
        ..Script::ok()
    };
    assert_op(s.land().unwrap_err(), "compaction land dispatch commit");
}
