//! **A nomination is judged by what it removes** (ARCH §2.7, bl-7234).
//!
//! `mark_for_deletion` stages `git rm -r`, so a nomination of an
//! ancestor sheds every not-eligible path beneath it while a predicate
//! reading the nominated string alone says nothing: `.` sheds the system
//! slot's three files and the lineage's `facts.md`, `messages` sheds the
//! dispatch entry, `summary` sheds the pass's own product. The one
//! predicate answers all three against the removal set
//! ([`super::eligibility`]), and these are its beats — in both
//! directions, because a directory holding nothing not-eligible must
//! still be sheddable whole or the compaction chain cannot be bounded.

use super::tests::{AGENT, repo_with};
use super::tests_own_product::{OneCallFails, commit_side_effect, nothing_staged};
use super::*;
use crate::prompt::dispatch::MESSAGES_DIR;
use crate::template::RealGit;
use std::cell::RefCell;

#[test]
fn a_failed_removal_set_read_surfaces_as_a_git_error() {
    // The removal set is read before either name-derived class can be
    // asked about a member (bl-7234), so its `ls-files` is a git call
    // the predicate owns and must report as one.
    let git = OneCallFails {
        fails: "ls-files",
        founding: "cafe1234\n".into(),
        seen: RefCell::new(Vec::new()),
    };
    let err = mark_for_deletion(
        std::path::Path::new("/nowhere"),
        AGENT,
        "summary/001.md",
        &git,
    )
    .unwrap_err();
    assert!(
        matches!(
            err,
            Error::Git {
                op: "mark_for_deletion removal set",
                ..
            }
        ),
        "{err:?}"
    );
}

#[test]
fn nominating_the_summary_directory_is_declined_for_this_passs_summary() {
    // The ancestor form of the third class. `git rm -r -- summary`
    // takes the summary this pass just wrote, and the pathspec the
    // own-product diff already uses resolves over the subtree — so the
    // decline fires and names the member, not the directory.
    let dir = repo_with("messages/002-user.md");
    let wt = dir.path();
    let rel = write_summary(wt, "the span so far\n").unwrap();
    commit_side_effect(wt);

    let err = mark_for_deletion(wt, AGENT, SUMMARY_DIR, &RealGit::new()).unwrap_err();
    assert!(
        matches!(&err, Error::NotCompactionEligible { path, .. } if path == SUMMARY_DIR),
        "{err:?}"
    );
    let text = err.to_string();
    assert!(text.contains(&rel), "{text}");
    assert!(
        text.contains("this compaction pass's own product"),
        "{text}"
    );
    assert!(wt.join(&rel).exists());
    nothing_staged(wt);
}

#[test]
fn nominating_the_summary_directory_still_supersedes_an_earlier_passs_chain() {
    // The other direction, and the reason the class is never a blanket
    // `summary/**` refusal: a directory holding only summaries this pass
    // inherited is eligible whole, so the chain stays bounded.
    let dir = repo_with("summary/001.md");
    let wt = dir.path();
    mark_for_deletion(wt, AGENT, SUMMARY_DIR, &RealGit::new()).unwrap();
    assert!(!wt.join("summary/001.md").exists());
}

#[test]
fn nominating_the_messages_directory_is_declined_for_the_dispatch_entry() {
    // The observed shape of the walk-around: the entry is refused by
    // name, so a model that keeps trying reaches for its parent. The
    // reason the entry is not history — it is the operator's only copy
    // of the opening prompt — does not weaken because the gesture got
    // coarser, so the whole nomination is declined and the transcript
    // survives intact.
    let dir = repo_with("messages/001-user.md");
    let wt = dir.path();
    std::fs::write(wt.join("messages/002-user.md"), "later\n").unwrap();
    commit_side_effect(wt);

    let err = mark_for_deletion(wt, AGENT, MESSAGES_DIR, &RealGit::new()).unwrap_err();
    assert!(
        matches!(&err, Error::NotCompactionEligible { path, .. } if path == MESSAGES_DIR),
        "{err:?}"
    );
    let text = err.to_string();
    assert!(text.contains("messages/001-user.md"), "{text}");
    assert!(text.contains("dispatch entry"), "{text}");
    assert!(wt.join("messages/001-user.md").exists());
    assert!(wt.join("messages/002-user.md").exists());
    nothing_staged(wt);
}

#[test]
fn nominating_the_worktree_root_is_declined_for_the_system_slot() {
    // `.` is the widest walk-around there is: `git rm -r -- .` empties
    // the branch, system slot and all, and the dispatching branch then
    // keeps stepping with no goal, no soul and no identity line.
    let dir = repo_with("goal.md");
    let wt = dir.path();
    let err = mark_for_deletion(wt, AGENT, ".", &RealGit::new()).unwrap_err();
    assert!(
        matches!(&err, Error::NotCompactionEligible { path, .. } if path == "."),
        "{err:?}"
    );
    let text = err.to_string();
    assert!(text.contains("goal.md"), "{text}");
    assert!(text.contains("system slot"), "{text}");
    assert!(wt.join("goal.md").exists());
    nothing_staged(wt);
}

#[test]
fn a_directory_with_nothing_not_eligible_under_it_is_shed_whole() {
    // The both-directions half: an ordinary work-product directory is
    // as nominable as any file in it, and the removal-set read costs it
    // nothing.
    let dir = repo_with("notes/scratch/a.md");
    let wt = dir.path();
    mark_for_deletion(wt, AGENT, "notes", &RealGit::new()).unwrap();
    assert!(!wt.join("notes/scratch/a.md").exists());
}
