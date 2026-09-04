//! The checkpoint-flush clock and span-selection arms of `run_flush`
//! (ARCH §2.6, §2.7, §6) — split from [`gate`](super::gate) to hold the
//! per-file line cap. The shared real-git harness lives in [`super`].

use super::super::run_flush;
use super::{Fx, workflow};
use crate::template::{GitRunner, RealGit};
use crate::workspace::{agent_worktree, fixture};

#[test]
fn run_flush_is_a_noop_when_the_clock_is_below_threshold() {
    // A `compaction:` block present but not yet due (§2.7): the git state is
    // derived, `due` is false, and no compactor is dispatched.
    let (_h, ws) = fixture::workspace();
    let parent = "20260101-g3";
    let wt = fixture::spawn_root(&ws, parent);
    let fx = Fx::new();
    let wf = workflow(
        "events: {}\ncompaction:\n  intermediate:\n    trigger: every_n_commits\n    n: 1000000\n",
    );
    run_flush(&ws, parent, &wt, &wf, &fx.deps()).unwrap();
    assert!(fx.launcher.launched.borrow().is_empty());
}

#[test]
fn run_flush_never_compacts_a_compactor_branch() {
    // THE PIN (§2.7): a compactor is not a member of the
    // compaction-eligible set. It is dispatched off a parent whose whole
    // history it inherits, so under the old root-relative count it read
    // the parent's commits as its own and re-tripped `every_n_commits`
    // immediately — the 226-branch cascade of bl-a9eb (yog bl-ebbd).
    // Both invariants are exercised here: the clock starts at the
    // compactor's own dispatch commit, and the role excludes it outright.
    let (_h, ws) = fixture::workspace();
    fixture::amend_config(&ws, &[("souls/compactor.md", "c")]);
    let parent = "20260101-g9";
    let parent_wt = fixture::spawn_root(&ws, parent);
    // Twenty commits of parent history for the compactor to inherit.
    for i in 0..20 {
        std::fs::write(parent_wt.join("goal.md"), format!("g{i}")).unwrap();
        RealGit::new().run(&parent_wt, &["add", "-A"]).unwrap();
        RealGit::new()
            .run(&parent_wt, &["commit", "-m", "step"])
            .unwrap();
    }
    // The parent IS due, and dispatching its compactor is the one launch.
    let fx = Fx::new();
    let wf = workflow(
        "events: {}\ncompaction:\n  intermediate:\n    trigger: every_n_commits\n    n: 1\n",
    );
    run_flush(&ws, parent, &parent_wt, &wf, &fx.deps()).unwrap();
    let compactor = fx.launcher.launched.borrow()[0].clone();

    // Now run the same boundary on the compactor itself: not due, no
    // second-generation compactor, no cascade.
    let compactor_wt = agent_worktree(&ws, &compactor);
    run_flush(&ws, &compactor, &compactor_wt, &wf, &fx.deps()).unwrap();
    assert_eq!(
        fx.launcher.launched.borrow().len(),
        1,
        "a compactor dispatches no compactor: {:?}",
        fx.launcher.launched.borrow()
    );
}

#[test]
fn run_flush_forks_the_compactor_off_the_configured_compaction_point() {
    // §2.6/§6 span selection: with `keep_recent: 2`, the compactor forks
    // off `HEAD~2` — the compaction point — so the retained tail is
    // structurally outside its view and survives the landing verbatim.
    let (_h, ws) = fixture::workspace();
    fixture::amend_config(&ws, &[("souls/compactor.md", "c")]);
    let parent = "20260101-g5";
    let parent_wt = fixture::spawn_root(&ws, parent);
    let git = RealGit::new();
    for i in 0..5 {
        std::fs::write(parent_wt.join("goal.md"), format!("g{i}")).unwrap();
        git.run(&parent_wt, &["add", "-A"]).unwrap();
        git.run(&parent_wt, &["commit", "-m", "step"]).unwrap();
    }
    let fx = Fx::new();
    let wf = workflow(
        "events: {}\ncompaction:\n  intermediate:\n    trigger: every_n_commits\n    n: 3\n    keep_recent: 2\n",
    );
    run_flush(&ws, parent, &parent_wt, &wf, &fx.deps()).unwrap();
    let compactor = fx.launcher.launched.borrow()[0].clone();
    // The basic agentic loop binds one dispatch, so one child forks —
    // unchanged by the checkpoint running every dispatch it binds
    // (`docs/DESIGN_LEARNING_LOOP.md` §2, bl-e6ed).
    assert_eq!(fx.launcher.launched.borrow().len(), 1);

    // The compactor's dispatch commit sits directly on HEAD~2.
    let fork_parent = git
        .run_capture(
            &parent_wt,
            &[
                "rev-parse",
                &format!("{}~1", crate::workspace::agent_ref(&compactor)),
            ],
        )
        .unwrap();
    let point = git
        .run_capture(&parent_wt, &["rev-parse", "HEAD~2"])
        .unwrap();
    assert_eq!(fork_parent, point, "forked off the compaction point");
}

/// A clock pinned far in the future, so an `every_t_seconds` trigger is
/// always due against freshly made commits. `now_unix` derives from
/// `now_iso8601` (one wall-clock source, `crate::prompt::clock`).
struct FarClock;
impl crate::prompt::Clock for FarClock {
    fn now_iso8601(&self) -> String {
        "2099-01-01T00:00:00Z".into()
    }
    fn now_compact(&self) -> String {
        "20990101T000000Z".into()
    }
}

#[test]
fn run_flush_skips_a_due_clock_whose_span_sits_inside_the_retained_tail() {
    // §2.6/§6: due on elapsed time, but every commit since the checkpoint
    // sits inside the retained tail — the span is empty, so no compactor
    // is dispatched and the clock simply stays due.
    let (_h, ws) = fixture::workspace();
    let parent = "20260101-g6";
    let wt = fixture::spawn_root(&ws, parent);
    let fx = Fx::new();
    let far = FarClock;
    let mut deps = fx.deps();
    deps.clock = &far;
    let wf = workflow(
        "events: {}\ncompaction:\n  intermediate:\n    trigger: every_t_seconds\n    n: 1\n    keep_recent: 4\n",
    );
    run_flush(&ws, parent, &wt, &wf, &deps).unwrap();
    assert!(fx.launcher.launched.borrow().is_empty());
}

#[test]
fn two_compactors_racing_to_one_summary_path_refuse_the_late_lander() {
    // bl-b9f0's collision half, and the answer ARCH's contract already
    // implies: **refuse the late lander**, never version the path and
    // never last-write. Two compactors forked off the same point each
    // number their summary `001` — each `next_seq` scans a directory the
    // other's write is not in — so if both landed, the second base would
    // silently overwrite the first's summary with a view of a span that
    // had already been squashed away. It cannot: the first landing
    // rebases the compaction point out of the branch, and the second
    // pass then cannot prove its own point is still reachable, so it is
    // superseded, lands nothing, and is not marked a defect (§2.6).
    use super::super::{has_pending_result, interpret_pending};
    use super::returned_child;
    let (_h, ws) = fixture::workspace();
    let parent = "20260101-g9";
    fixture::spawn_root(&ws, parent);
    let fx = Fx::new();
    let first = returned_child(
        &ws,
        parent,
        "compactor",
        "compact",
        ("summary/001.md", "the first pass's view\n"),
        &fx,
    );
    let second = returned_child(
        &ws,
        parent,
        "compactor",
        "compact",
        ("summary/001.md", "the second pass's view\n"),
        &fx,
    );
    assert_ne!(first, second, "two distinct passes");

    let wt = agent_worktree(&ws, parent);
    let wf = workflow("events: {}\n");
    // Both results are pending; one interpretation pass lands the first
    // and meets the second already overtaken.
    interpret_pending(&ws, parent, &wt, &wf, &fx.deps()).unwrap();

    // Exactly one compaction base on the branch: the loser landed nothing.
    // Which pass wins is the inbox read order and is not the subject —
    // that exactly one does, and that the surviving summary is its
    // author's text verbatim rather than either an overwrite or a merge,
    // is.
    let bases = fx
        .git
        .run_capture(
            &wt,
            &[
                "log",
                "--format=%s",
                "-E",
                "--grep",
                "^compaction base \\[",
                "HEAD",
            ],
        )
        .unwrap();
    let subjects: Vec<&str> = bases.lines().filter(|l| !l.trim().is_empty()).collect();
    assert_eq!(subjects.len(), 1, "one base, not two: {subjects:?}");
    let winner = subjects[0]
        .trim_start_matches("compaction base [")
        .trim_end_matches(']');
    let expected = if winner == first {
        "the first pass's view\n"
    } else {
        assert_eq!(winner, second, "{subjects:?}");
        "the second pass's view\n"
    };
    assert_eq!(
        std::fs::read_to_string(wt.join("summary/001.md")).unwrap(),
        expected,
        "the landed summary is its own author's, never the late lander's"
    );
    // Superseded is not a defect: neither pass is marked conflicted, and
    // both triggers are consumed (re-reading one would re-attempt a
    // settled outcome).
    for id in [&first, &second] {
        assert!(
            fx.git
                .run_capture(&wt, &["rev-parse", &format!("refs/litany/conflicted/{id}")])
                .is_err(),
            "an overtaken pass is not a defect: {id}"
        );
    }
    assert!(!has_pending_result(&ws, parent).unwrap(), "both consumed");
}

#[test]
fn a_superseded_compactor_return_lands_nothing_and_is_consumed() {
    // §2.6 superseded at the interpreter: a landing sits inside the
    // returning compactor's replay span (another pass overtook it), so
    // nothing lands, nothing is marked, and the trigger is consumed.
    use super::super::{has_pending_result, interpret_pending};
    use super::returned_child;
    let (_h, ws) = fixture::workspace();
    let parent = "20260101-g7";
    fixture::spawn_root(&ws, parent);
    let fx = Fx::new();
    let child = returned_child(
        &ws,
        parent,
        "compactor",
        "compact",
        ("summary/001.md", "s\n"),
        &fx,
    );
    let wt = agent_worktree(&ws, parent);
    std::fs::write(wt.join("x.txt"), "x\n").unwrap();
    fx.git.run(&wt, &["add", "-A"]).unwrap();
    fx.git
        .run(&wt, &["commit", "-m", "compaction base [other]"])
        .unwrap();
    let before = fx.git.run_capture(&wt, &["rev-parse", "HEAD"]).unwrap();

    interpret_pending(&ws, parent, &wt, &workflow("events: {}\n"), &fx.deps()).unwrap();

    assert_eq!(
        fx.git.run_capture(&wt, &["rev-parse", "HEAD"]).unwrap(),
        before,
        "nothing landed"
    );
    assert!(
        fx.git
            .run_capture(
                &wt,
                &["rev-parse", &format!("refs/litany/conflicted/{child}")]
            )
            .is_err(),
        "an overtaken pass is not a defect"
    );
    assert!(
        !has_pending_result(&ws, parent).unwrap(),
        "trigger consumed"
    );
}
