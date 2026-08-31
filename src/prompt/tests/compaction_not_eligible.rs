//! **A conversation past a compaction still has its opening prompt, and
//! still has a goal, a soul and a name** (ARCH §2.7 *what is written at
//! dispatch is not compaction-eligible*; bl-898f, bl-541b).
//!
//! Written from the operator's sentence, not from the mechanism: run a
//! conversation through a real compaction — a compactor forked off the
//! checkpoint commit, nominating through the real `mark_for_deletion`,
//! its product landed by the §6 hop — and assert the branch's
//! `messages/001-*` is still on disk **and still renders**, i.e. still
//! composes into the next model call's wire history (§5.2).
//!
//! Before the fix it did not. The opening prompt is written twice at
//! dispatch — as `goal.md` (§2.8) and, through the front door (§2.11), as
//! the dispatch entry `messages/001-user.md` — and the compactor's own
//! goal quotes `goal.md` verbatim (§2.7), so the one entry a model told to
//! nominate superseded files reads as *pure duplication* is the one the
//! operator reads. It was nominated, marked, squashed into the compaction
//! base, and gone.
//!
//! The system slot's three files (`goal.md`, `soul.md`, `name` — §5.2)
//! sit in the same range and were never observed going, but are strictly
//! worse when they do: a compactor writes its own three at its dispatch
//! commit, so a nomination after that is a `D` in `dispatch..tip`, which
//! the landing classifies as the compactor's product and applies to the
//! dispatching branch. That branch then keeps stepping with no goal, no
//! soul or no identity line on every later model call.
//!
//! The fifth nomination is the pass's **own summary** (bl-c7bb), and it
//! was seen accepted in the wild in the same tool sequence that wrote
//! it. The landing admits the summary and the deletions and nothing
//! else (§2.6), so accepting it lands a base holding no record of the
//! span at all. This test drives all five nominations and asserts every
//! file survives the landing — the summary among them, read back off
//! the dispatching branch after the rebase-forward.

use super::advance::{AGENT, RecLauncher, worker_config};
use super::fixtures::*;
use crate::prompt::Clock;
use crate::prompt::child_dispatch::{ChildDispatchRequest, run as dispatch_child};
use crate::prompt::compactor::tools;
use crate::prompt::dispatch::advance::{AdvanceOutcome, run};
use crate::prompt::inbox::{self, deposit_result};
use crate::prompt::{Error, PinnedDocs};
use crate::template::{GitRunner, RealGit};
use crate::workspace::agent_name::mint::test_rng;
use crate::workspace::{agent_worktree, fixture};

/// The branch's opening prompt — the text the operator typed, which
/// `goal.md` and the dispatch entry both carry (§2.8, §2.11).
const OPENING: &str = "port the parser to the new grammar\n";

/// A hyphen-free compact stamp so a dispatched child's id is a clean
/// two-token descent segment (§2.3), as [`super::advance_compaction`]'s.
struct DescentClock;
impl Clock for DescentClock {
    fn now_iso8601(&self) -> String {
        "iso".into()
    }
    fn now_compact(&self) -> String {
        "ct1".into()
    }
}

#[test]
fn a_conversation_past_a_compaction_still_has_its_opening_prompt_goal_soul_and_name() {
    let (_h, ws) = fixture::workspace();
    let parent = AGENT;
    let parent_wt = fixture::spawn_root(&ws, parent);
    let git = RealGit::new();
    let (clock, id) = (FixedClock::default(), FixedIdGen);
    let rec = RecLauncher::default();

    // The dispatching branch at the checkpoint commit: its opening prompt
    // as the dispatch entry, its goal beside it (the two projections of
    // one dispatch), and one later exchange for the compactor to shed.
    std::fs::create_dir_all(parent_wt.join("messages")).unwrap();
    std::fs::write(parent_wt.join("messages/001-user.md"), OPENING).unwrap();
    std::fs::write(parent_wt.join("messages/002-user.md"), "any progress?\n").unwrap();
    // The system slot's three, as a real dispatch commit writes them
    // (§2.3 step 2) — this test's other subject is that all three are
    // still here after the landing.
    std::fs::write(parent_wt.join("goal.md"), OPENING).unwrap();
    std::fs::write(parent_wt.join("soul.md"), "you are a worker\n").unwrap();
    std::fs::write(parent_wt.join("name"), "swift-heron\n").unwrap();
    git.run(&parent_wt, &["add", "-A"]).unwrap();
    git.run(&parent_wt, &["commit", "-m", "checkpoint"])
        .unwrap();

    // A real compactor child, forked off that commit: its worktree
    // inherits the dispatching branch's transcript (§2.7, no dialog prune).
    let child = dispatch_child(
        &ChildDispatchRequest {
            repo: &ws,
            parent_branch: parent,
            parent_worktree: &parent_wt,
            role: "compactor",
            goal: "compact",
            name: None,
            fork_point: None,
            cwd: None,
            pins: PinnedDocs::none(),
        },
        &git,
        &DescentClock,
        &id,
        &rec,
        test_rng(),
    )
    .unwrap();
    let cwt = agent_worktree(&ws, &child);
    assert_eq!(
        std::fs::read_to_string(cwt.join("messages/001-user.md")).unwrap(),
        OPENING,
        "the compactor inherits the entry it used to delete"
    );

    // The compaction itself, through the compactor's real toolset. The
    // nomination the shipped model actually made is declined in-band and
    // stages nothing; the later entry is shed as it always was.
    tools::write_summary(&cwt, "the parser port is underway\n").unwrap();
    // The pass's own product, staged as the harness stages a tool's side
    // effect (§2.3): the live run accepted exactly this nomination, and
    // the landing admits the summary and the deletions and nothing else
    // (§2.6) — so it would have carried away the one artifact standing
    // in for the whole compacted span (bl-c7bb).
    git.run(&cwt, &["add", "-A"]).unwrap();
    let declined = tools::mark_for_deletion(&cwt, &child, "summary/001.md", &git).unwrap_err();
    assert!(
        matches!(&declined, Error::NotCompactionEligible { path, what }
            if path == "summary/001.md" && what.contains("own product")),
        "{declined:?}"
    );
    let declined =
        tools::mark_for_deletion(&cwt, &child, "messages/001-user.md", &git).unwrap_err();
    assert!(
        matches!(&declined, Error::NotCompactionEligible { path, .. }
            if path == "messages/001-user.md"),
        "{declined:?}"
    );
    for slot in crate::prompt::dispatch::step_commit::SYSTEM_SLOT_FILES {
        let declined = tools::mark_for_deletion(&cwt, &child, slot, &git).unwrap_err();
        assert!(
            matches!(&declined, Error::NotCompactionEligible { path, .. } if path == slot),
            "{slot}: {declined:?}"
        );
    }
    tools::mark_for_deletion(&cwt, &child, "messages/002-user.md", &git).unwrap();
    git.run(&cwt, &["add", "-A"]).unwrap();
    git.run(&cwt, &["commit", "-m", "compaction"]).unwrap();
    let tip = git.run_capture(&cwt, &["rev-parse", "HEAD"]).unwrap();

    // The compactor returns; the hop lands the rebase-forward and steps.
    deposit_result(
        &ws,
        parent,
        &child,
        inbox::Epitaph::FinalResponse,
        tip.trim(),
        Some("compacted"),
        &clock,
        &git,
    )
    .unwrap();
    inbox::deposit(&ws, parent, "user", "carry on", &clock).unwrap();

    let adapter = StubAdapter::scripted([StubAdapter::reply_ok(&happy_response_bytes())]);
    let (sleeper, tools_stub, stub_git) = (
        StubSleeper::default(),
        StubToolExecutor::ok(),
        StubGit::ok(),
    );
    let mut deps = valid_deps(&adapter, &sleeper, &stub_git, &clock, &id, &tools_stub, &ws);
    deps.git = &git;
    deps.launcher = &rec;
    let out = run(&ws, parent, None, &deps, &mut || Ok(worker_config())).unwrap();
    assert!(matches!(out, AdvanceOutcome::Terminal), "the step ran");

    // A compaction really landed — summary in, the later entry squashed out.
    assert_eq!(
        std::fs::read_to_string(parent_wt.join("summary/001.md")).unwrap(),
        "the parser port is underway\n"
    );
    assert!(!parent_wt.join("messages/002-user.md").exists());

    // The dispatching branch still has a goal, a soul and a name — the
    // three the landing would have `git rm`'d had the nomination taken.
    for slot in crate::prompt::dispatch::step_commit::SYSTEM_SLOT_FILES {
        assert!(parent_wt.join(slot).exists(), "{slot} survived the landing");
    }

    // The operator's copy of the opening prompt is still there …
    assert_eq!(
        std::fs::read_to_string(parent_wt.join("messages/001-user.md")).unwrap(),
        OPENING
    );
    // … and still renders: it composes into the wire history of the step
    // the branch took after the landing (§5.2), read off the step record.
    let req: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(ws.join(format!("steps/{parent}/001/request.json"))).unwrap(),
    )
    .unwrap();
    assert!(
        req["messages"].to_string().contains("port the parser"),
        "{}",
        req["messages"]
    );
}
