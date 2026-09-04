//! The **learning loop** asset (`docs/DESIGN_LEARNING_LOOP.md` §2): the
//! named alternative `litany prime` seeds beside the basic agentic loop.
//! Split out of `mod.rs` to keep that file under the per-file line cap.
//!
//! Two facts are pinned here and they answer different questions. The
//! *seeding* half is `prime`'s contract — the file is in the pool, and a
//! curated one survives a re-prime (§2.2). The *relationship* half is why
//! a second asset is lawful at all: `workflows/learning-loop.yaml` is a
//! separate file rather than a derivation of `template/workflow.yaml`,
//! because composing it at seed time would drop that file's comments (a
//! pool entry is copied and read by people) or patch YAML text by hand.
//! A second copy of a declaration drifts unless something holds it, so
//! [`the_learning_loop_is_the_basic_loop_plus_the_reviewer`] is that
//! something: every block but `events:` must be equal, and `events:` must
//! differ by exactly the two reviewer bindings.

use super::*;
use crate::config::{Action, Event, Workflow};

/// The two shipped declarations, parsed: the basic agentic loop (the
/// embedded config template's `workflow.yaml`, which is also what `prime`
/// seeds as `basic-agentic-loop.yaml`) and the learning loop.
fn shipped() -> (Workflow, Workflow) {
    let basic_raw = crate::template::TEMPLATE
        .get_file("workflow.yaml")
        .expect("the template ships workflow.yaml")
        .contents_utf8()
        .expect("workflow.yaml is UTF-8");
    let basic = Workflow::parse(basic_raw, Path::new("template/workflow.yaml"))
        .expect("the basic agentic loop parses");
    let learning = Workflow::parse(
        LEARNING_LOOP_YAML,
        Path::new("workflows/learning-loop.yaml"),
    )
    .expect("the learning loop parses");
    (basic, learning)
}

/// The learning loop is the basic loop plus the reviewer, and nothing
/// else — the drift guard the second asset is lawful under.
#[test]
fn the_learning_loop_is_the_basic_loop_plus_the_reviewer() {
    let (basic, learning) = shipped();

    // Every block but `events:`: the compaction clock the reviewer rides,
    // the retry policy, the tool-output bounds, the (absent) budgets and
    // tool control. Compared by substituting the events map, so a new
    // field on `Workflow` is covered without being named here.
    let mut rest = learning.clone();
    rest.events = basic.events.clone();
    assert_eq!(
        rest, basic,
        "learning-loop.yaml diverged from the basic agentic loop outside \
         its events: block — re-derive it from template/workflow.yaml"
    );

    // `events:` differs by exactly two bindings.
    for (event, actions) in &basic.events {
        let mut expected = actions.clone();
        if *event == Event::WorkerFlush {
            expected.push("dispatch(reviewer)".into());
        }
        assert_eq!(
            Some(&expected),
            learning.events.get(event),
            "{} diverged",
            event.as_str()
        );
    }
    assert_eq!(
        learning.actions_for(Event::ReviewerReturn),
        vec![Action::StageProposal],
        "the reviewer's landing is the one binding the learning loop adds"
    );
    let added: Vec<_> = learning
        .events
        .keys()
        .filter(|e| !basic.events.contains_key(e))
        .collect();
    assert_eq!(added, vec![&Event::ReviewerReturn], "one event was added");
}

/// `prime` seeds the alternative beside the default (§2.2, §6), and a
/// curated file survives a re-prime like every other pool entry.
#[test]
fn prime_seeds_the_learning_loop_beside_the_default() {
    let home = TempDir::new().unwrap();
    prime(&collapsed(home.path())).unwrap();

    let pool = home.path().join(WORKFLOWS_DIR);
    assert!(pool.join(BASIC_AGENTIC_LOOP).is_file());
    let seeded = pool.join(LEARNING_LOOP);
    assert_eq!(fs::read_to_string(&seeded).unwrap(), LEARNING_LOOP_YAML);

    fs::write(&seeded, "events: {}\n").unwrap();
    prime(&collapsed(home.path())).unwrap();
    assert_eq!(fs::read_to_string(&seeded).unwrap(), "events: {}\n");
}

/// The seeded default is not the learning loop: the basic agentic loop
/// changes by zero bytes (`docs/DESIGN_WORKFLOW_SWITCH.md` §3), so the
/// reviewer costs an install that never opts in exactly nothing.
#[test]
fn the_default_still_dispatches_no_reviewer() {
    let (basic, _) = shipped();
    assert_eq!(
        basic.actions_for(Event::WorkerFlush),
        vec![Action::Dispatch {
            role: "compactor".into(),
            with: None,
            mode: None,
        }],
        "the basic agentic loop forks a compactor and nobody else"
    );
    assert!(basic.actions_for(Event::ReviewerReturn).is_empty());
}
