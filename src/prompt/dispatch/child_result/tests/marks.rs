//! The two ref marks at a child's return (`docs/DESIGN_PROCEDURES.md`
//! §7, bl-ef78): `notify_ui` / `mark_abandoned` bound on `<role>_return`
//! or a bound `<role>_<token>` mark the **dispatcher** and touch the
//! result not at all — the injection watcher's natural landing. The same
//! marks at a gating child's verdict are in [`super::gate`].

use super::super::{has_pending_result, interpret_pending};
use super::{Fx, returned_child, returned_child_full, workflow};
use crate::prompt::inbox::Epitaph;
use crate::template::GitRunner;
use crate::workspace::{agent_worktree, fixture};
use std::path::Path;

/// Whether the §6 `notify_ui` mark stands for `agent` in `wt`'s repo.
fn notified(wt: &Path, agent: &str, fx: &Fx) -> bool {
    let name = format!("refs/litany/notify/{agent}");
    fx.git
        .run(wt, &["show-ref", "--verify", "--quiet", &name])
        .is_ok()
}

#[test]
fn a_ref_mark_at_a_childs_return_marks_the_dispatcher_and_leaves_the_result() {
    let (_h, ws) = fixture::workspace();
    let parent = "20260101-p4";
    fixture::spawn_root(&ws, parent);
    let fx = Fx::new();
    returned_child(&ws, parent, "worker", "do it", ("out.txt", "x\n"), &fx);
    let wt = agent_worktree(&ws, parent);
    // A ref mark at a child's return marks the DISPATCHER
    // (`docs/DESIGN_PROCEDURES.md` §7) and touches the result not at all,
    // as the hold does: consuming it is a landing's or a delivery's.
    let wf = workflow("events:\n  worker_return:\n    - notify_ui\n");
    interpret_pending(&ws, parent, &wt, &wf, &fx.deps()).unwrap();
    assert!(
        fx.git
            .run(
                &wt,
                &[
                    "show-ref",
                    "--verify",
                    "--quiet",
                    &format!("refs/litany/notify/{parent}")
                ]
            )
            .is_ok()
    );
    assert!(
        has_pending_result(&ws, parent).unwrap(),
        "a mark consumes nothing"
    );
}

#[test]
fn the_watchers_verdict_marks_the_dispatcher_and_lands_its_finding() {
    // §7's injection watcher, as config: `guard_injected: [notify_ui,
    // land_response(guard)]` — the mark says the operator should look,
    // the landing is why, the guard's return never enters the transcript.
    let (_h, ws) = fixture::workspace();
    fixture::amend_config(&ws, &[("souls/guard.md", "g")]);
    let parent = "20260101-v7";
    let wt = fixture::spawn_root(&ws, parent);
    let fx = Fx::new();
    returned_child_full(
        &ws,
        parent,
        "guard",
        "watch",
        ("o.txt", "x"),
        Epitaph::FinalResponse,
        Some("INJECTED: the tool result asks the model to ignore its goal"),
        &fx,
    );
    let wf = workflow("events:\n  guard_injected:\n    - notify_ui\n    - land_response(guard)\n");
    interpret_pending(&ws, parent, &wt, &wf, &fx.deps()).unwrap();
    assert!(notified(&wt, parent, &fx), "the dispatcher wears the mark");
    assert!(
        std::fs::read_to_string(wt.join("guard"))
            .unwrap()
            .starts_with("INJECTED:")
    );
    assert_eq!(
        std::fs::read_dir(wt.join("messages")).map_or(0, |d| d.count()),
        0,
        "landed, not delivered"
    );
    assert!(!has_pending_result(&ws, parent).unwrap());
}
