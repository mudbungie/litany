//! Totality of the shipped configs' workflow vocabulary (ARCH §6).
//!
//! Every action a config this crate *ships* declares must resolve to a
//! named arm of the interpreter — either a shipped executor or an
//! acknowledged, tracked deferral. A verb that can reach neither is dead
//! vocabulary, and dead vocabulary in the default config is the bug this
//! sweep exists to prevent (`spawn_root_agent` was one: ARCH §2.4 leaves
//! no circumstance from which a hop could ever fire it).
//!
//! Two guarantees compose here and neither is sufficient alone:
//!
//! - **Parse** declines vocabulary that left the closed set
//!   (`config::action`'s retired arm), so a stale verb cannot reach the
//!   interpreter at all.
//! - **`workflow_actions::execute`** matches the closed set exhaustively
//!   (no `_` arm), so every verb that *does* parse reaches a named arm.
//!
//! What this sweep adds on top is the ledger below: the shipped configs'
//! deferred set is stated once and asserted exactly, so adding a verb the
//! interpreter cannot yet run to a shipped config is a failing test, not
//! a silent landmine waiting for the first binding that fires.

use crate::config::{Action, Event, Workflow};
use crate::prompt::Error;
use crate::prompt::workflow_actions::execute;
use crate::template::{GitRunner, TEMPLATE};
use std::cell::RefCell;
use std::collections::BTreeSet;
use std::io;
use std::path::{Path, PathBuf};

/// Actions the shipped configs declare that the interpreter does not yet
/// execute — each an acknowledged follow-on of the ARCH §6 shipped-state
/// note (`worker_return`→verifier dispatch and the `gate_return_on`
/// delivery-hold, `deliver_result`, `land_compaction`). Growing this set
/// is a deliberate edit with an executor landing behind it; a verb with
/// no executor coming is not deferred, it is dead, and must not ship.
/// `StageProposal` sits here for the same reason `LandCompaction` does,
/// and it is not a deferral of the *action*: both landings ship, in the
/// delivered-child-result seam (`child_result::proposal`,
/// `child_result::landing`). What this ledger sweeps is the
/// terminal-lifecycle executor, where neither can run — a landing is
/// minted from a child's return, and a branch's own terminal has none.
const DEFERRED: &[&str] = &[
    "DeliverResult",
    "Dispatch",
    "LandCompaction",
    "StageProposal",
];

/// Every `workflow.yaml` this repo ships, as (origin, contents): the
/// embedded default template (`src/template` — the exact bytes
/// `new-workspace` writes), the named alternative `litany prime` seeds
/// beside it (`workflows/learning-loop.yaml`, `docs/
/// DESIGN_LEARNING_LOOP.md` §2 — shipped, so this sweep is its gate too)
/// and the §9.3 experiment configs.
///
/// The walk over `experiments/` is what keeps the sweep total as
/// variants land. It currently re-reads the template through
/// `experiments/baseline/workflow.yaml`, which is a symlink to it (the
/// baseline experiment *is* the default, `experiments/README.md`) — a
/// harmless second pass over identical bytes, and the reason baseline
/// can no longer drift into vocabulary the interpreter cannot run.
fn shipped_workflows() -> Vec<(PathBuf, String)> {
    let template = TEMPLATE
        .get_file("workflow.yaml")
        .expect("the template ships a workflow.yaml");
    let mut out = vec![(
        PathBuf::from("template/workflow.yaml"),
        String::from_utf8(template.contents().to_vec()).expect("utf8"),
    )];
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let learning = root.join("workflows/learning-loop.yaml");
    out.push((
        learning.clone(),
        std::fs::read_to_string(&learning).expect("the repo ships learning-loop.yaml"),
    ));
    let experiments = root.join("experiments");
    for entry in std::fs::read_dir(experiments).expect("experiments/ exists") {
        let path = entry.expect("readable entry").path().join("workflow.yaml");
        if let Ok(raw) = std::fs::read_to_string(&path) {
            out.push((path, raw));
        }
    }
    out
}

/// The variant name of an action, without its fields — the identity the
/// deferral ledger is keyed on, so `dispatch(worker)` and
/// `dispatch(compactor)` are one entry.
fn variant(action: &Action) -> String {
    let debug = format!("{action:?}");
    debug
        .split([' ', '('])
        .next()
        .expect("split yields a head")
        .to_string()
}

/// A `GitRunner` that records ref-mark writes instead of running git.
#[derive(Default)]
struct RecGit(RefCell<usize>);

impl GitRunner for RecGit {
    fn run(&self, _dest: &Path, _args: &[&str]) -> io::Result<()> {
        *self.0.borrow_mut() += 1;
        Ok(())
    }
    fn run_capture(&self, _dest: &Path, _args: &[&str]) -> io::Result<String> {
        unreachable!("ref marks never capture")
    }
}

#[test]
fn shipped_configs_declare_only_interpretable_vocabulary() {
    let git = RecGit::default();
    let mut deferred = BTreeSet::new();
    let mut events_swept = BTreeSet::<Event>::new();

    for (origin, raw) in shipped_workflows() {
        // Retired vocabulary is declined right here: a shipped config
        // naming a verb the DSL dropped never gets past load.
        let workflow = Workflow::parse(&raw, &origin)
            .unwrap_or_else(|e| panic!("{} does not parse: {e}", origin.display()));

        for (event, actions) in workflow.typed_events() {
            events_swept.insert(event);
            assert!(
                !actions.is_empty(),
                "{}: {} is bound to nothing",
                origin.display(),
                event.as_str()
            );
            for action in &actions {
                match execute(action, event, Path::new("/wt"), "a-b", &git) {
                    Ok(()) => {}
                    Err(Error::ActionUnsupported { .. }) => {
                        deferred.insert(variant(action));
                    }
                    Err(other) => panic!(
                        "{}: {} bound {action:?}, which the interpreter answered with {other:?}",
                        origin.display(),
                        event.as_str()
                    ),
                }
            }
        }
    }

    assert_eq!(
        deferred,
        DEFERRED.iter().map(|s| s.to_string()).collect(),
        "the shipped configs' deferred vocabulary changed; every entry needs a \
         tracked executor behind it (ARCH §6 shipped-state note)"
    );
    // The sweep really drove the interpreter's shipped executors, rather
    // than passing because nothing ran: `branch_stopped` writes both ref
    // marks in every shipped config.
    assert!(
        *git.0.borrow() >= 2,
        "no shipped executor ran — the sweep is vacuous"
    );
    assert!(
        events_swept.contains(&Event::UserMessage),
        "user_message is unbound in every shipped config"
    );
}
