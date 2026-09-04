//! The retarget mark is consumed **by the agent's own executor, at a step
//! boundary** (ARCH §2.2, §2.3) — the claim the whole design rests on,
//! proven here against a real workspace by driving one `litany advance`
//! hop over a marked branch.
//!
//! Nothing about the hop is special-cased for it: the branch has an empty
//! transcript, so the hop's warrant is `NothingDue` and it exits without a
//! model call (the injected resolver is a tripwire that must never run).
//! The retarget still landed, because the boundary consumes the mark
//! *before* anything resolves config — which is what makes the next step
//! the first the target governs.

use super::advance::no_resolve;
use super::fixtures::{FixedClock, FixedIdGen, never_stopped, no_launch};
use super::stubs::{StubSleeper, unreachable_adapter};
use super::tool_stub::StubToolExecutor;
use crate::prompt::Deps;
use crate::prompt::dispatch::advance::{AdvanceOutcome, run};
use crate::template::RealGit;
use crate::workspace::{self, agent_ref};
use std::path::Path;

#[test]
fn the_executor_lands_a_marked_retarget_at_its_next_step_boundary() {
    let (_h, ws, wt) = crate::prompt::retarget::tests::agent();
    let git = RealGit::new();
    // A diverged lineage: under follow-the-tip (§2.2, bl-403b) a
    // same-lineage advance reaches the agent by resolution alone, so
    // what a retarget lands is a change of lineage.
    let target =
        crate::prompt::retarget::tests::variant(&ws, &[("souls/worker.md", "the newer soul\n")]);
    let before = workspace::governing_config(&ws, &agent_ref("a"), &git).unwrap();
    assert_ne!(before.trim(), target, "the lineage change is real");
    workspace::retarget::write(&ws, "a", &target, &git).unwrap();

    let adapter = unreachable_adapter();
    let tools = StubToolExecutor::ok();
    let sleeper = StubSleeper::default();
    let clock = FixedClock::default();
    let id_gen = FixedIdGen;
    let deps = Deps {
        adapter: &adapter,
        sleeper: &sleeper,
        git: &git,
        clock: &clock,
        id_gen: &id_gen,
        tool_executor: &tools,
        config_root: Path::new("/nonexistent"),
        data_root: Path::new("/nonexistent"),
        adapter_target: None,
        stop: never_stopped(),
        launcher: no_launch(),
        rng: crate::workspace::agent_name::mint::test_rng(),
    };
    let outcome = run(&ws, "a", None, &deps, &mut no_resolve).unwrap();

    // The hop itself was an ordinary no-op — no model call, no resolve.
    assert!(
        matches!(outcome, AdvanceOutcome::NothingToDo),
        "{outcome:?}"
    );
    // …and the branch is now governed by the target, with its own
    // dispatch commit re-derived on top of it and the mark consumed.
    assert_eq!(
        workspace::governing_config(&ws, &agent_ref("a"), &git)
            .unwrap()
            .trim(),
        target,
    );
    assert_eq!(workspace::retarget::read(&ws, "a", &git), None);
    assert_eq!(
        std::fs::read_to_string(wt.join("soul.md")).unwrap(),
        "the newer soul",
    );
}
