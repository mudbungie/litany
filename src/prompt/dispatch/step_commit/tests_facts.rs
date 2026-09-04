//! The trim's fourth part: the lineage's **facts file** is derived from
//! the governing config commit at every fork (`crate::facts`, ARCH
//! §5.5) — never inherited from the dispatcher's tree. Split from
//! [`super::tests`] for the per-file line cap.

use crate::facts::FILE;
use crate::prompt::Error;
use crate::prompt::dispatch::{Grant, trim_to_context};
use crate::template::{GitRunner, RealGit};
use crate::workspace::{config_ref, fixture, repo_git};
use std::path::Path;

fn config_tip(ws: &Path) -> String {
    RealGit::new()
        .run_capture(&repo_git(ws), &["rev-parse", &config_ref("default")])
        .unwrap()
}

fn trim(wt: &Path, commit: &str, git: &dyn GitRunner) -> Result<(), Error> {
    let grant = Grant {
        role: "worker",
        tools: &[],
        config_commit: commit,
    };
    trim_to_context(wt, "20260101-f1", &grant, None, git)
}

#[test]
fn a_commit_carrying_facts_yields_a_tree_carrying_them() {
    let (_h, ws) = fixture::workspace();
    fixture::amend_config(&ws, &[(FILE, "the build runs on nightly\n")]);
    let wt = fixture::spawn_root(&ws, "a1");

    trim(&wt, &config_tip(&ws), &RealGit::new()).unwrap();

    assert_eq!(
        std::fs::read_to_string(wt.join(FILE)).unwrap(),
        "the build runs on nightly\n"
    );
    // Tracked, so the dispatch commit carries it (§2.3 step 2) and
    // the manifest's `pinned: [facts.md]` rule can see it.
    let tracked = RealGit::new()
        .run_capture(&wt, &["ls-files", "--", FILE])
        .unwrap();
    assert_eq!(tracked.trim(), FILE);
}

#[test]
fn a_child_gets_the_followed_commits_bytes_over_its_parents_stale_copy() {
    // The staleness the re-cut prices: the parent forked before the
    // fact was edited, so its tree carries the old bytes. A child
    // derives from the commit, never from what it forked off.
    let (_h, ws) = fixture::workspace();
    fixture::amend_config(&ws, &[(FILE, "old\n")]);
    let parent = fixture::spawn_root(&ws, "a1");
    assert_eq!(std::fs::read_to_string(parent.join(FILE)).unwrap(), "old\n");
    fixture::amend_config(&ws, &[(FILE, "new\n")]);
    let child = fixture::spawn_agent(&ws, "a1-c1", "agents/a1");

    trim(&child, &config_tip(&ws), &RealGit::new()).unwrap();

    assert_eq!(std::fs::read_to_string(child.join(FILE)).unwrap(), "new\n");
    // `git checkout <commit> -- <path>` stages as it writes, so the
    // dispatch commit carries the rewrite with no second `add`.
    let staged = RealGit::new()
        .run_capture(&child, &["diff", "--cached", "--name-status"])
        .unwrap();
    assert!(
        staged.lines().any(|l| l == format!("M\t{FILE}")),
        "{staged:?}"
    );
}

#[test]
fn absent_in_the_commit_is_absent_in_the_tree() {
    // A fork governed by a commit that carries no facts file, off a
    // tree that does: the inherited copy is removed rather than
    // kept, so the tree is a function of the commit alone.
    let (_h, ws) = fixture::workspace();
    let before = config_tip(&ws);
    fixture::amend_config(&ws, &[(FILE, "old\n")]);
    let wt = fixture::spawn_root(&ws, "a1");
    assert!(wt.join(FILE).exists());

    trim(&wt, &before, &RealGit::new()).unwrap();

    assert!(!wt.join(FILE).exists());
    let staged = RealGit::new()
        .run_capture(&wt, &["diff", "--cached", "--name-status"])
        .unwrap();
    assert!(
        staged.lines().any(|l| l == format!("D\t{FILE}")),
        "{staged:?}"
    );
}

#[test]
fn no_facts_anywhere_is_a_no_op() {
    // Every lineage that has authored no fact: nothing in the
    // commit, nothing in the tree, no git command at all.
    let (_h, ws) = fixture::workspace();
    let wt = fixture::spawn_root(&ws, "a1");
    trim(&wt, &config_tip(&ws), &RealGit::new()).unwrap();
    assert!(!wt.join(FILE).exists());
}

#[test]
fn a_refused_cut_surfaces_as_a_named_git_error() {
    struct RefusesFactsCheckout(RealGit);
    impl GitRunner for RefusesFactsCheckout {
        fn run(&self, dest: &Path, args: &[&str]) -> std::io::Result<()> {
            if args.first() == Some(&"checkout") && args.contains(&FILE) {
                return Err(std::io::Error::other("checkout refused"));
            }
            self.0.run(dest, args)
        }
        fn run_capture(&self, dest: &Path, args: &[&str]) -> std::io::Result<String> {
            self.0.run_capture(dest, args)
        }
    }
    let (_h, ws) = fixture::workspace();
    fixture::amend_config(&ws, &[(FILE, "a fact\n")]);
    let wt = fixture::spawn_root(&ws, "a1");
    let err = trim(&wt, &config_tip(&ws), &RefusesFactsCheckout(RealGit::new())).unwrap_err();
    assert!(
        matches!(&err, Error::Git { op, .. } if *op == "cut the facts file"),
        "{err}"
    );
}
