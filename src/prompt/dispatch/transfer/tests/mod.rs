//! Tests for the work-product transfer at delivery (ARCH §2.6).
//!
//! Behavioral arms (happy apply, empty diff, declined transfer,
//! bad-ref merge-base) run against a **real** git repo so the filtered
//! diff and its application are exercised end-to-end. The remaining
//! git-op error arms route through a stub that fails a chosen verb, the
//! same `failing_at` pattern the sibling drain tests use.

use super::*;
use crate::prompt::Error;
use crate::template::{GitRunner, RealGit};
use std::cell::RefCell;
use std::io;
use tempfile::TempDir;

// ---- terminal_ref_of --------------------------------------------------

#[test]
fn terminal_ref_of_reads_the_frontmatter_field() {
    let body =
        "---\nfrom: a-b\ndeposited_at: t\nepitaph: final-response\nterminal_ref: abc123\n---\nhi";
    assert_eq!(terminal_ref_of(body).as_deref(), Some("abc123"));
}

#[test]
fn terminal_ref_of_is_none_without_frontmatter_or_field() {
    // No leading `---`.
    assert_eq!(terminal_ref_of("terminal_ref: x\n"), None);
    // Frontmatter closes before the field appears.
    assert_eq!(terminal_ref_of("---\nfrom: a\n---\nterminal_ref: x"), None);
    // An ordinary steering message: frontmatter, no terminal_ref.
    assert_eq!(terminal_ref_of("---\nfrom: user\n---\nhello"), None);
    // Present but empty value is not a ref.
    assert_eq!(terminal_ref_of("---\nterminal_ref:   \n---\n"), None);
    // Frontmatter opens but never closes and has no field — the scan
    // runs off the end and returns None.
    assert_eq!(terminal_ref_of("---\nfrom: a\nunterminated"), None);
}

// ---- real-git behavioral arms ----------------------------------------

fn git() -> RealGit {
    RealGit::new()
}

/// A repo on `main` with one `fork` commit; every later branch forks from
/// it. Returns the TempDir (holding the worktree at its root).
fn init_repo() -> TempDir {
    let dir = TempDir::new().unwrap();
    let wt = dir.path();
    let g = git();
    g.run(wt, &["init", "-b", "main"]).unwrap();
    g.run(wt, &["config", "user.email", "t@t"]).unwrap();
    g.run(wt, &["config", "core.hooksPath", "/dev/null"])
        .unwrap();
    g.run(wt, &["config", "user.name", "t"]).unwrap();
    std::fs::write(wt.join("base.txt"), "base\n").unwrap();
    g.run(wt, &["add", "-A"]).unwrap();
    g.run(wt, &["commit", "-m", "fork"]).unwrap();
    dir
}

fn write(wt: &std::path::Path, rel: &str, content: &str) {
    let path = wt.join(rel);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, content).unwrap();
}

/// Fork a `child` branch off the fork commit, write the given files,
/// commit, return the terminal sha, and check `main` back out.
fn make_child(wt: &std::path::Path, files: &[(&str, &str)]) -> String {
    let g = git();
    g.run(wt, &["checkout", "-b", "child"]).unwrap();
    for (rel, content) in files {
        write(wt, rel, content);
    }
    g.run(wt, &["add", "-A"]).unwrap();
    g.run(wt, &["commit", "-m", "child work"]).unwrap();
    let terminal = g.run_capture(wt, &["rev-parse", "HEAD"]).unwrap();
    g.run(wt, &["checkout", "main"]).unwrap();
    terminal
}

#[test]
fn apply_lands_only_work_products_as_one_commit() {
    let dir = init_repo();
    let wt = dir.path();
    // Child edits a work product and its own context; only the work
    // product transfers (§2.6 exclusions).
    let terminal = make_child(
        wt,
        &[
            ("feature.txt", "feature\n"),
            ("messages/001-x.md", "ctx\n"),
            ("summary/001.md", "sum\n"),
            ("skills/s/SKILL.md", "skill\n"),
            ("goal.md", "child goal\n"),
        ],
    );

    apply(wt, "p-child", &terminal, &git()).unwrap();

    // The work product landed on main; the context paths did not.
    assert_eq!(
        std::fs::read_to_string(wt.join("feature.txt")).unwrap(),
        "feature\n"
    );
    assert!(!wt.join("messages").exists());
    assert!(!wt.join("summary").exists());
    assert!(!wt.join("skills").exists());
    assert!(!wt.join("goal.md").exists());
    // One transfer commit, subject naming the child.
    let subject = git()
        .run_capture(wt, &["log", "-1", "--pretty=%s"])
        .unwrap();
    assert_eq!(subject, "work-product transfer [p-child]");
}

#[test]
fn apply_commits_nothing_when_only_context_changed() {
    let dir = init_repo();
    let wt = dir.path();
    let terminal = make_child(
        wt,
        &[("messages/001-x.md", "ctx\n"), ("summary/001.md", "s\n")],
    );

    apply(wt, "p-child", &terminal, &git()).unwrap();

    // No transfer commit — HEAD is still the fork commit.
    let subject = git()
        .run_capture(wt, &["log", "-1", "--pretty=%s"])
        .unwrap();
    assert_eq!(subject, "fork");
    assert!(!wt.join("messages").exists());
}

#[test]
fn apply_declines_loudly_when_the_diff_does_not_apply() {
    let dir = init_repo();
    let wt = dir.path();
    // Child adds feature.txt.
    let terminal = make_child(wt, &[("feature.txt", "child version\n")]);
    // Parent independently created the same path with other content — a
    // write-path violation (harness defect). The add-patch cannot apply.
    write(wt, "feature.txt", "parent version\n");
    git().run(wt, &["add", "-A"]).unwrap();
    git().run(wt, &["commit", "-m", "parent diverged"]).unwrap();

    apply(wt, "p-child", &terminal, &git()).unwrap();

    // Declined: no transfer commit, and the conflicted ref points at the
    // child's terminal sha (every byte preserved on its branch).
    let subject = git()
        .run_capture(wt, &["log", "-1", "--pretty=%s"])
        .unwrap();
    assert_eq!(subject, "parent diverged");
    let marked = git()
        .run_capture(wt, &["rev-parse", "refs/litany/conflicted/p-child"])
        .unwrap();
    assert_eq!(marked, terminal);
    // Parent's own version is untouched by the declined apply.
    assert_eq!(
        std::fs::read_to_string(wt.join("feature.txt")).unwrap(),
        "parent version\n"
    );
}

#[test]
fn apply_surfaces_a_bad_terminal_ref_as_merge_base_error() {
    let dir = init_repo();
    let wt = dir.path();
    let err = apply(wt, "p-child", "deadbeefdeadbeef", &git()).unwrap_err();
    assert!(
        matches!(
            err,
            Error::Git {
                op: "transfer merge-base",
                ..
            }
        ),
        "got {err:?}"
    );
}

// ---- stub-git error arms ---------------------------------------------

/// Stub that dispatches on the git verb so each transfer op-index test
/// forces exactly one failure. On the `diff` call it writes the patch to
/// the `--output=` path (non-empty) so control flow reaches `apply` and
/// beyond, mirroring what real `git diff` would produce.
#[derive(Default)]
struct StubGit {
    fail_diff: bool,
    apply_fails: bool,
    fail_commit: bool,
    fail_update_ref: bool,
    invocations: RefCell<Vec<String>>,
}

impl GitRunner for StubGit {
    fn run(&self, _dest: &std::path::Path, args: &[&str]) -> io::Result<()> {
        self.invocations.borrow_mut().push(args[0].to_string());
        match args[0] {
            "diff" => {
                if self.fail_diff {
                    return Err(io::Error::other("diff boom"));
                }
                let out = args
                    .iter()
                    .find_map(|a| a.strip_prefix("--output="))
                    .expect("diff carries --output");
                std::fs::write(out, "diff --git a/f b/f\n").unwrap();
                Ok(())
            }
            "apply" if self.apply_fails => Err(io::Error::other("apply boom")),
            "apply" => Ok(()),
            "commit" if self.fail_commit => Err(io::Error::other("commit boom")),
            "update-ref" if self.fail_update_ref => Err(io::Error::other("ref boom")),
            _ => Ok(()),
        }
    }
    fn run_capture(&self, _dest: &std::path::Path, args: &[&str]) -> io::Result<String> {
        self.invocations.borrow_mut().push(args[0].to_string());
        // Only merge-base is captured; return a plausible fork sha.
        Ok("f0f0f0f0".to_string())
    }
}

fn wt() -> TempDir {
    TempDir::new().unwrap()
}

#[test]
fn apply_surfaces_diff_failure() {
    let d = wt();
    let git = StubGit {
        fail_diff: true,
        ..Default::default()
    };
    let err = apply(d.path(), "c", "term", &git).unwrap_err();
    assert!(
        matches!(
            err,
            Error::Git {
                op: "transfer diff",
                ..
            }
        ),
        "got {err:?}"
    );
}

#[test]
fn apply_surfaces_commit_failure() {
    let d = wt();
    let git = StubGit {
        fail_commit: true,
        ..Default::default()
    };
    let err = apply(d.path(), "c", "term", &git).unwrap_err();
    assert!(
        matches!(
            err,
            Error::Git {
                op: "transfer commit",
                ..
            }
        ),
        "got {err:?}"
    );
}

#[test]
fn apply_declines_and_surfaces_update_ref_failure() {
    let d = wt();
    let git = StubGit {
        apply_fails: true,
        fail_update_ref: true,
        ..Default::default()
    };
    let err = apply(d.path(), "c", "term", &git).unwrap_err();
    assert!(
        matches!(
            err,
            Error::Git {
                op: "transfer decline update-ref",
                ..
            }
        ),
        "got {err:?}"
    );
    // The decline path was taken: apply ran, then update-ref.
    assert!(git.invocations.borrow().iter().any(|c| c == "update-ref"));
}

mod descriptions;
mod name;
