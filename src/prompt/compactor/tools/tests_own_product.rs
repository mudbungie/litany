//! **A compaction pass may not shed its own product** (ARCH §2.7, the
//! third not-compaction-eligible class; bl-c7bb).
//!
//! The observed run, in order: `write_summary {"content": …}` →
//! `{"status":"written","path":"summary/001.md"}`, then
//! `mark_for_deletion {"path":"summary/001.md"}` →
//! `{"status":"marked","path":"summary/001.md"}`. Both accepted. The
//! landing admits the summary and the deletions and nothing else (§2.6),
//! so that pair lands a `git rm` of the one artifact standing in for the
//! whole compacted span.
//!
//! The class is *this run's* output, never the `summary/` directory:
//! superseding an earlier pass's summary is what the shipped soul
//! instructs and what keeps the chain bounded, so both directions are
//! asserted here.

use super::tests::{AGENT, repo_with};
use super::*;
use crate::template::RealGit;
use std::cell::RefCell;

/// Commit the worktree the way the harness commits a tool's side effect
/// (§2.3, §3.3 — `git add -A` with the tool result).
pub(super) fn commit_side_effect(wt: &std::path::Path) {
    let g = RealGit::new();
    g.run(wt, &["add", "-A"]).unwrap();
    g.run(wt, &["commit", "-m", "tool: write_summary"]).unwrap();
}

/// Nothing is staged for the next commit — the decline removed nothing.
pub(super) fn nothing_staged(wt: &std::path::Path) {
    let staged = RealGit::new()
        .run_capture(wt, &["diff", "--cached", "--name-status"])
        .unwrap();
    assert!(staged.trim().is_empty(), "nothing staged: {staged:?}");
}

#[test]
fn the_summary_this_pass_just_wrote_is_declined() {
    // The live repro, end to end at the toolset: write, let the harness
    // commit it, then nominate it. The decline is in-band, names the
    // class, and leaves the file exactly where the landing needs it.
    let dir = repo_with("messages/002-user.md");
    let wt = dir.path();
    let rel = write_summary(wt, "the parser port is underway\n").unwrap();
    assert_eq!(rel, "summary/001.md");
    commit_side_effect(wt);

    let err = mark_for_deletion(wt, AGENT, &rel, &RealGit::new()).unwrap_err();
    assert!(
        matches!(&err, Error::NotCompactionEligible { path, .. } if *path == rel),
        "{err:?}"
    );
    let text = err.to_string();
    assert!(text.contains("summary/001.md"), "{text}");
    assert!(
        text.contains("this compaction pass's own product"),
        "{text}"
    );
    assert!(text.contains("not compaction-eligible"), "{text}");
    assert!(wt.join(&rel).exists(), "the summary is still on disk");
    nothing_staged(wt);
}

#[test]
fn a_staged_but_uncommitted_summary_is_already_this_passs_product() {
    // Read against the index, so the guard does not wait on the tool
    // step's commit: staged is already carried away by a `git rm`.
    let dir = repo_with("messages/002-user.md");
    let wt = dir.path();
    let rel = write_summary(wt, "body\n").unwrap();
    RealGit::new().run(wt, &["add", "-A"]).unwrap();
    let err = mark_for_deletion(wt, AGENT, &rel, &RealGit::new()).unwrap_err();
    assert!(
        matches!(&err, Error::NotCompactionEligible { path, .. } if *path == rel),
        "{err:?}"
    );
    assert!(wt.join(&rel).exists());
}

#[test]
fn an_untracked_summary_is_taken_by_the_existence_decline_and_survives() {
    // The one shape the index read does not see is the one `git rm`
    // cannot remove either, so the file survives on the older decline
    // and no third rule is needed for it.
    let dir = repo_with("messages/002-user.md");
    let wt = dir.path();
    let rel = write_summary(wt, "body\n").unwrap();
    let err = mark_for_deletion(wt, AGENT, &rel, &RealGit::new()).unwrap_err();
    assert!(
        matches!(
            err,
            Error::Git {
                op: "mark_for_deletion rm",
                ..
            }
        ),
        "{err:?}"
    );
    assert!(wt.join(&rel).exists(), "the summary is still on disk");
    nothing_staged(wt);
}

#[test]
fn an_earlier_passs_summary_is_still_nominable() {
    // The counter-direction, and the reason a blanket `summary/**`
    // refusal is wrong: a summary the compactor *inherited* — present at
    // its own dispatch commit — is exactly what the soul tells it to
    // supersede, and the removal stages like any other.
    let dir = repo_with("summary/001.md");
    let wt = dir.path();
    mark_for_deletion(wt, AGENT, "summary/001.md", &RealGit::new()).unwrap();
    assert!(!wt.join("summary/001.md").exists());
    let staged = RealGit::new()
        .run_capture(wt, &["diff", "--cached", "--name-status"])
        .unwrap();
    assert!(staged.starts_with('D'), "staged deletion: {staged:?}");
}

#[test]
fn a_summary_seq_this_pass_rewrote_is_this_passs_product() {
    // The edge the add/modify filter exists for: a pass that supersedes
    // `summary/001.md` and then writes its own into the freed seq holds
    // its OWN content at that path. Inherited-ness is a property of the
    // blob, not of the name, so the second nomination is declined.
    let dir = repo_with("summary/001.md");
    let wt = dir.path();
    mark_for_deletion(wt, AGENT, "summary/001.md", &RealGit::new()).unwrap();
    let rel = write_summary(wt, "this pass's view\n").unwrap();
    assert_eq!(rel, "summary/001.md");
    commit_side_effect(wt);

    let err = mark_for_deletion(wt, AGENT, &rel, &RealGit::new()).unwrap_err();
    assert!(
        matches!(&err, Error::NotCompactionEligible { path, .. } if *path == rel),
        "{err:?}"
    );
    assert_eq!(
        std::fs::read_to_string(wt.join(&rel)).unwrap(),
        "this pass's view\n"
    );
}

#[test]
fn a_tree_with_no_dispatch_commit_has_no_product_of_its_own() {
    // The general path with empty inputs (never a bootstrap case):
    // nothing was added after a commit that does not exist, so the class
    // is empty and the two name-derived classes answer alone.
    let dir = tempfile::TempDir::new().unwrap();
    let (wt, git) = (dir.path(), RealGit::new());
    git.run(wt, &["init", "-b", "agents/p1"]).unwrap();
    git.run(wt, &["config", "user.email", "t@t"]).unwrap();
    git.run(wt, &["config", "core.hooksPath", "/dev/null"])
        .unwrap();
    git.run(wt, &["config", "user.name", "t"]).unwrap();
    std::fs::write(wt.join("keep.txt"), "x").unwrap();
    git.run(wt, &["add", "-A"]).unwrap();
    git.run(wt, &["commit", "-m", "unfounded"]).unwrap();
    let rel = write_summary(wt, "body\n").unwrap();
    assert!(
        not_compaction_eligible(wt, AGENT, &rel, &git)
            .unwrap()
            .is_none()
    );
}

/// A runner that answers every git call the predicate makes ahead of
/// `fails`, and fails that one — so each git arm of the predicate is
/// reachable without breaking the lookups in front of it.
pub(super) struct OneCallFails {
    pub(super) fails: &'static str,
    pub(super) founding: String,
    pub(super) seen: RefCell<Vec<String>>,
}
impl GitRunner for OneCallFails {
    fn run(&self, _dir: &std::path::Path, _args: &[&str]) -> std::io::Result<()> {
        unreachable!("the decline precedes every write")
    }
    fn run_capture(&self, _dir: &std::path::Path, args: &[&str]) -> std::io::Result<String> {
        self.seen.borrow_mut().push(args.join(" "));
        match args.first() {
            Some(v) if *v == self.fails => Err(std::io::Error::other("boom")),
            Some(&"log") => Ok(self.founding.clone()),
            _ => Ok(String::new()),
        }
    }
}

#[test]
fn a_failed_own_product_diff_surfaces_as_a_git_error() {
    let git = OneCallFails {
        fails: "diff",
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
                op: "mark_for_deletion own-product diff",
                ..
            }
        ),
        "{err:?}"
    );
    let seen = git.seen.borrow();
    assert!(seen.iter().any(|a| a.contains("cafe1234")), "{seen:?}");
}
