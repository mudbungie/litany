//! Coverage for excluding `descriptions/**` from the transfer (bl-475a).
//!
//! `descriptions/**` is config-inherited context (ARCH §2.2), not a
//! branch-scoped one, but a child's dispatch commit prunes it to the
//! child's own role grant (§3.3 "The fork prunes the snapshot to the
//! role's grant") — a harness-driven deletion, not agent-authored work.
//! Without the exclusion those deletions ride the fork-point→terminal
//! diff back into the parent and silently shrink its own toolset.

use super::super::apply;
use super::{git, init_repo, write};
use crate::template::GitRunner;

/// Seed the fork point (already on `main`, the tip `init_repo` leaves)
/// with two tool descriptions, as a real config commit would (§3.3
/// descriptions-always).
fn seed_descriptions(wt: &std::path::Path) {
    let g = git();
    write(wt, "descriptions/tools/bash.json", "{}\n");
    write(wt, "descriptions/tools/slack_post.json", "{}\n");
    g.run(wt, &["add", "-A"]).unwrap();
    g.run(wt, &["commit", "-m", "seed descriptions"]).unwrap();
}

/// Fork a `child` branch, simulate its dispatch commit pruning
/// `slack_post` out of its role grant (§3.3) alongside a genuine work
/// product, commit, return the terminal sha, and check `main` back out.
fn make_pruning_child(wt: &std::path::Path, feature_contents: &str) -> String {
    let g = git();
    g.run(wt, &["checkout", "-b", "child"]).unwrap();
    g.run(wt, &["rm", "descriptions/tools/slack_post.json"])
        .unwrap();
    write(wt, "feature.txt", feature_contents);
    g.run(wt, &["add", "-A"]).unwrap();
    g.run(wt, &["commit", "-m", "dispatch prune + child work"])
        .unwrap();
    let terminal = g.run_capture(wt, &["rev-parse", "HEAD"]).unwrap();
    g.run(wt, &["checkout", "main"]).unwrap();
    terminal
}

#[test]
fn apply_excludes_descriptions_so_a_childs_role_prune_never_reaches_the_parent() {
    let dir = init_repo();
    let wt = dir.path();
    seed_descriptions(wt);
    let terminal = make_pruning_child(wt, "feature\n");

    apply(wt, "p-child", &terminal, &git()).unwrap();

    // The work product landed on main.
    assert_eq!(
        std::fs::read_to_string(wt.join("feature.txt")).unwrap(),
        "feature\n"
    );
    // The parent's own descriptions/** survived the child's role-grant
    // prune — it is context management, not a work product (bl-475a).
    assert!(wt.join("descriptions/tools/bash.json").exists());
    assert!(wt.join("descriptions/tools/slack_post.json").exists());
}

#[test]
fn apply_declines_on_a_real_conflict_even_when_the_child_also_pruned_descriptions() {
    let dir = init_repo();
    let wt = dir.path();
    seed_descriptions(wt);
    let terminal = make_pruning_child(wt, "child version\n");

    // Parent independently created the same work-product path — a
    // write-path violation (harness defect), same shape as the sibling
    // decline test in `mod.rs`. The add-patch cannot apply.
    write(wt, "feature.txt", "parent version\n");
    git().run(wt, &["add", "-A"]).unwrap();
    git().run(wt, &["commit", "-m", "parent diverged"]).unwrap();

    apply(wt, "p-child", &terminal, &git()).unwrap();

    // Declined on the real conflict alone: the descriptions prune, being
    // excluded from the diff entirely, is never a candidate cause and
    // never blocks or corrupts the decline.
    let subject = git()
        .run_capture(wt, &["log", "-1", "--pretty=%s"])
        .unwrap();
    assert_eq!(subject, "parent diverged");
    let marked = git()
        .run_capture(wt, &["rev-parse", "refs/litany/conflicted/p-child"])
        .unwrap();
    assert_eq!(marked, terminal);
    // The parent's descriptions/** is untouched by the decline either.
    assert!(wt.join("descriptions/tools/bash.json").exists());
    assert!(wt.join("descriptions/tools/slack_post.json").exists());
}
