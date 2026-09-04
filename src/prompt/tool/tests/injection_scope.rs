//! The **scope** of the host injection seam, as bl-a00a inverted it: an
//! installed host is the executor for every name, and the §3.3 three-hop
//! binary resolution stands behind it for none (`docs/
//! DESIGN_TOOL_INJECTION.md` §3.4).
//!
//! Its sibling [`super::injection`] asserts the *shape* of a routed
//! answer; this file asserts that there is nothing else to be. The sharp
//! case is a tool binary genuinely installed in the harness root, called
//! by name, and answered by the host anyway.
//!
//! Its third test was a *fan* answered by the host in list order; the
//! fan went with the `parallel` multi-tool envelope
//! (`docs/DESIGN_CODE_EXECUTION.md` §5), and what it pinned about
//! ordering is now a program's own business.

use super::super::{SpawnTool, ToolCall, ToolExecutor};
use super::fixtures::{FixedClock, HarnessRoot, StepDir, driver_target};
use super::injection::Embedder;
use serde_json::json;
use std::sync::atomic::AtomicBool;

#[test]
fn an_installed_host_answers_a_name_an_installed_binary_would_have() {
    // The inversion (bl-a00a). `greet` is installed in the harness root
    // and would resolve at the first hop, and the host does not own the
    // name — and it is still the host that answers. No fall-through, so
    // the local executor cannot be reached behind an installed injection
    // by picking a name nobody routed.
    let root = HarnessRoot::new();
    root.install("greet", r#"printf hello"#);
    let clock = FixedClock::default();
    let step = StepDir::new();
    let host = Embedder::new("teleop");
    let exec = SpawnTool::new(root.path(), &clock, driver_target()).with_injection(Some(&host));
    let outcome = exec
        .execute(
            ToolCall {
                id: "toolu_local",
                name: "greet",
                input: &json!({}),
            },
            &step.path,
            &AtomicBool::new(false),
            None,
        )
        .unwrap();
    assert!(outcome.is_error, "an unowned name refuses in band");
    assert_eq!(
        String::from_utf8_lossy(&outcome.content),
        "Exit code: 127\n--- stderr ---\nno such tool: greet",
    );
    assert_eq!(host.seen.borrow().len(), 1, "the host saw the invocation");
}

#[test]
fn an_unresolvable_caller_declines_before_anything_is_routed() {
    // A step dir the §2.2 shape cannot be read from fails `prepare`, and
    // preparation runs before routing, so the host is never asked about
    // a call that has no caller.
    let root = HarnessRoot::new();
    let clock = FixedClock::default();
    let host = Embedder::new("teleop");
    let exec = SpawnTool::new(root.path(), &clock, driver_target()).with_injection(Some(&host));
    let input = json!({});
    let result = exec.execute(
        ToolCall {
            id: "f_nowhere",
            name: "teleop",
            input: &input,
        },
        std::path::Path::new("/nonexistent/steps/agent/001"),
        &AtomicBool::new(false),
        None,
    );
    assert!(
        matches!(result, Err(super::super::ExecError::NoWorktree { .. })),
        "an unresolvable caller declines before anything is routed"
    );
    assert!(host.seen.borrow().is_empty());
}
