//! Where a terminal response goes when the dispatcher is not the one who
//! prompted it (ARCH §2.6 — *a reply answers the last prompter*), end to
//! end on the real child path: `litany advance` at a dispatched child,
//! with the deposit and the wake-up both observed on disk.
//!
//! [`super::parent_revival`] covers the same seam when the dispatcher
//! *is* the last prompter — the rule's first case, and the shape every
//! ordinary dispatch takes. This file covers the cases the bl-a96a
//! defect got wrong.

use super::parent_revival::{DescentClock, advance_child, dispatched_child};
use crate::prompt::dispatch::advance::AdvanceOutcome;
use crate::prompt::inbox::{self, Launcher, inbox_dir};
use std::io;
use std::path::Path;

/// A sibling of the dispatched child under test — an ordinary agent id
/// (§2.3 descent), not this child's dispatcher.
const SIBLING: &str = "20260101-a1-ct2-cafe";

/// Recording launcher: the launch targets, in order.
#[derive(Default)]
struct RecLauncher {
    invocations: std::cell::RefCell<Vec<String>>,
}
impl Launcher for RecLauncher {
    fn launch(&self, _ws: &Path, agent: &str) -> io::Result<()> {
        self.invocations.borrow_mut().push(agent.to_string());
        Ok(())
    }
}

/// A launcher that cannot spawn: §2.11 fire-and-forget says the failure
/// is logged and swallowed, never propagated.
struct FailingLauncher;
impl Launcher for FailingLauncher {
    fn launch(&self, _ws: &Path, _agent: &str) -> io::Result<()> {
        Err(io::Error::other("spawn refused"))
    }
}

/// The files pending in `agent`'s inbox, as bodies.
fn pending(ws: &Path, agent: &str) -> Vec<String> {
    let Ok(rd) = std::fs::read_dir(inbox_dir(ws, agent)) else {
        return Vec::new();
    };
    rd.flatten()
        .map(|e| std::fs::read_to_string(e.path()).unwrap())
        .collect()
}

#[test]
fn a_reply_goes_to_the_agent_that_prompted_the_step_not_the_dispatcher() {
    // The bl-a96a incident, one level over: somebody other than the
    // dispatcher put the question, so the answer is theirs. The
    // dispatcher hears nothing and is not woken — it asked nothing.
    let (_holder, ws, parent, _parent_wt, child) = dispatched_child();
    inbox::deposit(&ws, &child, SIBLING, "what about X?", &DescentClock).unwrap();

    let launcher = RecLauncher::default();
    let out = advance_child(&ws, &child, &launcher);
    assert!(matches!(out, AdvanceOutcome::Terminal), "{out:?}");

    // Deposited into the prompter's inbox, carrying the ordinary result
    // fields (§2.6) — the epitaph and terminal ref are pinned whoever
    // the recipient is.
    let reply = pending(&ws, SIBLING);
    assert_eq!(reply.len(), 1, "one reply, to the agent that asked");
    assert!(reply[0].contains("epitaph: final-response"), "{reply:?}");
    assert!(reply[0].contains(&format!("from: {child}")), "{reply:?}");
    assert!(
        pending(&ws, parent).is_empty(),
        "the dispatcher asked nothing this step and hears nothing"
    );
    // §2.11 exit protocol: the self-directed launch, then the recipient
    // the deposit landed in — the *same* address, never `parent_of`.
    assert_eq!(
        *launcher.invocations.borrow(),
        vec![child.clone(), SIBLING.to_string()]
    );
}

#[test]
fn a_failing_revival_launch_is_swallowed() {
    // §2.11 accepted crash class at the *recipient* side of the exit
    // protocol: the deposit still landed, and the stranding is late,
    // never lost — the next touch delivers it.
    let (_holder, ws, parent, _parent_wt, child) = dispatched_child();
    let out = advance_child(&ws, &child, &FailingLauncher);
    assert!(matches!(out, AdvanceOutcome::Terminal), "{out:?}");
    assert_eq!(pending(&ws, parent).len(), 1, "the deposit landed anyway");
}
