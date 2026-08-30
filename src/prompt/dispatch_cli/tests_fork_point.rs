//! `litany dispatch --from <ref>` where the fork point crosses config
//! lineages (ARCH §2.2, §2.3, §7.2).
//!
//! The property under test is the one §2.2 states — *"an agent started
//! by fork-back-in (§2.3) inherits its source's config the same way"* —
//! and it is a property about **agreement**: a child's control is read
//! at dispatch time from one commit and resolved at every later step
//! (`litany advance`, §6) from its own branch's ancestry. Those are the
//! same commit only if the dispatch reads the *fork point's* governing
//! config rather than the dispatcher's. Split from [`super::tests`] for
//! the per-file line cap — as are the same-lineage `--from` cases
//! below, which belong with them.

use super::tests::{NoopLauncher, scaffolded_repo_with_parent, sub_count};
use super::*;
use crate::template::{GitRunner, RealGit};
use crate::workspace::fixture;

/// A second config lineage forked off `default`, carrying its own worker
/// soul — the shape `litany config --from` authors (§2.2).
const STRICT_SOUL: &str = "You are the strict worker.";

fn author_strict(ws: &Path) {
    crate::template::authoring::author(
        ws,
        &ws.join(".no-pools"),
        "strict",
        crate::template::authoring::Origin::Fork { source: "default" },
        |dir| std::fs::write(dir.join("souls/worker.md"), STRICT_SOUL),
        &RealGit::new(),
    )
    .unwrap();
}

fn child_of(repo: &Path, parent: &str) -> String {
    std::fs::read_dir(repo.join(crate::workspace::AGENTS_DIR))
        .unwrap()
        .flatten()
        .map(|e| e.file_name().to_string_lossy().to_string())
        .find(|n| n.starts_with(&format!("{parent}-")))
        .expect("the forked child")
}

#[test]
fn a_child_forked_across_lineages_is_governed_by_the_config_it_forked_off() {
    let (_holder, repo) = scaffolded_repo_with_parent("20260101-p1");
    author_strict(&repo);
    // An agent on the other lineage; its tip is the fork point.
    fixture::spawn_agent(&repo, "20260101-r2", "config/strict");

    run_with(
        "worker",
        &repo,
        "20260101-p1",
        Some("continue that work"),
        Some("agents/20260101-r2"),
        None,
        crate::prompt::PinnedDocs::none(),
        None,
        &NoopLauncher,
    )
    .unwrap();

    let child = child_of(&repo, "20260101-p1");
    let git = RealGit::new();
    // The pinned soul is the *fork point's* lineage, not the
    // dispatcher's: the dispatch commit read the config that will govern
    // this child (§2.2), not the one that governs its parent.
    let soul =
        std::fs::read_to_string(crate::workspace::agent_worktree(&repo, &child).join("soul.md"))
            .unwrap();
    assert_eq!(soul, STRICT_SOUL);

    // And the agreement itself: the commit the artifacts came from is the
    // commit every later `litany advance` derives from the child's own
    // branch (§6, `resolve::ConfigSource::Agent`). One fact, one answer —
    // §4.3's "it must be the *same* commit the grant came from".
    let at_dispatch = crate::workspace::governing_config(&repo, "config/strict", &git).unwrap();
    let at_advance =
        crate::workspace::governing_config(&repo, &crate::workspace::agent_ref(&child), &git)
            .unwrap();
    assert_eq!(at_advance, at_dispatch);
    assert_ne!(
        at_advance,
        crate::workspace::governing_config(
            &repo,
            &crate::workspace::agent_ref("20260101-p1"),
            &git
        )
        .unwrap(),
        "the fork point's config must differ from the parent's, or this proves nothing"
    );
}

#[test]
fn role_validity_is_asked_of_the_config_that_will_govern_the_child() {
    // A role the *parent's* lineage defines and the fork point's does
    // not: the check must decline, because the soul would be read from a
    // commit that does not carry it (§4.3).
    let (_holder, repo) = scaffolded_repo_with_parent("20260101-p1");
    author_strict(&repo);
    // Drop `compactor` from the strict lineage only.
    crate::template::authoring::author(
        &repo,
        &repo.join(".no-pools"),
        "strict",
        crate::template::authoring::Origin::Advance,
        |dir| {
            std::fs::write(
                dir.join("providers.yaml"),
                "roles:\n  worker:\n    provider: anthropic\n    model: sonnet\n",
            )
        },
        &RealGit::new(),
    )
    .unwrap();
    fixture::spawn_agent(&repo, "20260101-r2", "config/strict");

    let err = run_with(
        ROLE_COMPACTOR,
        &repo,
        "20260101-p1",
        None,
        Some("agents/20260101-r2"),
        None,
        crate::prompt::PinnedDocs::none(),
        None,
        &NoopLauncher,
    )
    .unwrap_err();
    let msg = err.to_string();
    assert!(matches!(err, DispatchCliError::InvalidRole(_)), "{msg}");
    assert!(msg.contains("defined roles: worker"), "{msg}");
    assert_eq!(
        sub_count(&repo, "20260101-p1"),
        0,
        "declined before the fork"
    );
}

#[test]
fn a_named_fork_point_forks_the_child_off_it_not_the_parents_tip() {
    // §2.3 *Any ref is a legal fork point* / §7.2: `--from` is the
    // ordinary fork with a ref argument. The child is still
    // `<parent>-<sub>`, so its return address is the dispatcher's
    // (§2.6). This fork point is a commit of the parent's own branch, so
    // the governing config is unchanged too; the cross-lineage case is
    // [`super::tests_fork_point`].
    let (_holder, repo) = scaffolded_repo_with_parent("20260101-p1");
    let git = crate::template::RealGit::new();
    let parent_wt = crate::workspace::agent_worktree(&repo, "20260101-p1");
    let earlier = git
        .run_capture(&parent_wt, &["rev-parse", "HEAD"])
        .unwrap()
        .trim()
        .to_string();
    // The parent advances past that commit: a file that exists only on
    // its tip is what tells the two fork points apart.
    std::fs::write(parent_wt.join("tip-only.md"), "later").unwrap();
    git.run(&parent_wt, &["add", "tip-only.md"]).unwrap();
    git.run(&parent_wt, &["commit", "-m", "later work"])
        .unwrap();

    run_with(
        "worker",
        &repo,
        "20260101-p1",
        Some("continue from there"),
        Some(&earlier),
        None,
        crate::prompt::PinnedDocs::none(),
        None,
        &NoopLauncher,
    )
    .unwrap();

    let child = std::fs::read_dir(repo.join(crate::workspace::AGENTS_DIR))
        .unwrap()
        .flatten()
        .map(|e| e.file_name().to_string_lossy().to_string())
        .find(|n| n.starts_with("20260101-p1-"))
        .expect("the forked child");
    let child_ref = crate::workspace::agent_ref(&child);
    let bare = crate::workspace::repo_git(&repo);
    assert!(
        git.run(
            &bare,
            &["merge-base", "--is-ancestor", &earlier, &child_ref]
        )
        .is_ok(),
        "the named ref must be an ancestor of the child"
    );
    assert!(
        !repo
            .join(crate::workspace::AGENTS_DIR)
            .join(&child)
            .join("tip-only.md")
            .exists(),
        "the child forked off the earlier commit, not the parent's tip"
    );
}

#[test]
fn an_absent_fork_point_is_declined_before_the_fork() {
    let (_holder, repo) = scaffolded_repo_with_parent("20260101-p1");
    let err = run_with(
        "worker",
        &repo,
        "20260101-p1",
        Some("g"),
        Some("agents/nope"),
        None,
        crate::prompt::PinnedDocs::none(),
        None,
        &NoopLauncher,
    )
    .unwrap_err();
    let msg = err.to_string();
    assert!(
        matches!(err, DispatchCliError::UnknownForkPoint(_)),
        "{msg}"
    );
    assert!(msg.contains("no ref or commit \"agents/nope\""), "{msg}");
    assert!(msg.contains("a child forks off the ref you name"), "{msg}");
    assert_eq!(sub_count(&repo, "20260101-p1"), 0, "no branch, no debris");
}
