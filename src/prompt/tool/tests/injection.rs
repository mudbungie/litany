//! The host injection seam at the executor (ARCH §3.3 *Host-injected
//! tools*, `docs/DESIGN_TOOL_INJECTION.md`): a test embedder that
//! declares a tool of its own and routes it.
//!
//! What is asserted is the claim the design makes — that a routed tool
//! is indistinguishable from a spawned one *downstream*: the same result
//! envelope, `is_error`, bounded projection and `input.json` /
//! `output.json` pair under the same directory. The difference is
//! upstream and total: while a host is installed it is the executor, so
//! nothing resolves or spawns for any name — including one an installed
//! binary would have answered.

use super::super::inject::{InjectedTool, RoutedCall, RoutedCapture, ToolInjection};
use super::super::{
    INPUT_FILE, OUTPUT_FILE, STEP_TOOLS_SUBDIR, SpawnTool, ToolCall, ToolExecutor, ToolInputRecord,
    ToolOutputRecord,
};
use super::fixtures::{FixedClock, HarnessRoot, StepDir, driver_target};
use serde_json::{Value, json};
use std::cell::RefCell;
use std::sync::atomic::AtomicBool;

/// A test embedder. It declares one tool, answers **every** invocation
/// (the trait is total), and records what each one carried — so the four
/// wire facts a subprocess reads from stdin and its environment can be
/// asserted to reach a router unchanged. A name it does not own is its
/// own in-band refusal, exactly as an absent binary is.
pub(super) struct Embedder {
    owns: &'static str,
    exit_code: i32,
    pub(super) seen: RefCell<Vec<Seen>>,
}

/// What one routed call carried across the seam — the wire facts the
/// router is handed, recorded so the test can assert them verbatim.
#[derive(Debug, PartialEq)]
pub(super) struct Seen {
    pub(super) id: String,
    pub(super) name: String,
    pub(super) input: Value,
    pub(super) agent: String,
    pub(super) cwd: std::path::PathBuf,
}

impl Embedder {
    pub(super) fn new(owns: &'static str) -> Self {
        Self {
            owns,
            exit_code: 0,
            seen: RefCell::new(Vec::new()),
        }
    }

    fn failing(owns: &'static str) -> Self {
        Self {
            exit_code: 7,
            ..Self::new(owns)
        }
    }
}

impl ToolInjection for Embedder {
    fn tools(&self, _workspace: &std::path::Path, _agent: &str) -> Vec<InjectedTool> {
        vec![InjectedTool {
            name: self.owns.to_string(),
            input_schema: json!({"type": "object"}),
            description: Some("the host's own tool".into()),
        }]
    }

    fn route(&self, call: RoutedCall<'_>) -> RoutedCapture {
        self.seen.borrow_mut().push(Seen {
            id: call.id.to_string(),
            name: call.name.to_string(),
            input: call.input.clone(),
            agent: call.agent.to_string(),
            cwd: call.cwd.to_path_buf(),
        });
        assert!(call.workspace.is_dir(), "the router is told a live root");
        assert!(call.cwd.is_dir(), "the router is told a live cwd");
        assert!(!call.stop.load(std::sync::atomic::Ordering::SeqCst));
        if call.name != self.owns {
            return RoutedCapture {
                stdout: Vec::new(),
                stderr: format!("no such tool: {}", call.name).into_bytes(),
                exit_code: 127,
            };
        }
        RoutedCapture {
            stdout: b"routed product".to_vec(),
            stderr: if self.exit_code == 0 {
                Vec::new()
            } else {
                b"endpoint vanished".to_vec()
            },
            exit_code: self.exit_code,
        }
    }
}

#[test]
fn a_routed_tool_answers_without_a_binary_and_lands_the_record() {
    // The harness root is empty and the driver target is a bare name
    // that would fail to spawn — so if this call resolved at all, it
    // would fail loudly rather than pass quietly.
    let root = HarnessRoot::new();
    let clock = FixedClock::default();
    let step = StepDir::new();
    let host = Embedder::new("teleop");
    let exec = SpawnTool::new(root.path(), &clock, driver_target()).with_injection(Some(&host));

    let outcome = exec
        .execute(
            ToolCall {
                id: "toolu_r1",
                name: "teleop",
                input: &json!({"do": "thing"}),
            },
            &step.path,
            &AtomicBool::new(false),
            None,
        )
        .unwrap();

    assert!(!outcome.is_error);
    assert_eq!(outcome.content, b"Exit code: 0\nrouted product");
    assert_eq!(
        *host.seen.borrow(),
        vec![Seen {
            id: "toolu_r1".to_string(),
            name: "teleop".to_string(),
            input: json!({"do": "thing"}),
            agent: super::fixtures::AGENT_ID.to_string(),
            // The caller's resolved working directory crosses the seam
            // (bl-ddaa): with no cd mark it is the worktree, the same
            // directory a spawned subprocess runs in (§3.3).
            cwd: step.worktree.clone(),
        }],
    );

    let dir = step.path.join(STEP_TOOLS_SUBDIR).join("toolu_r1");
    let input: ToolInputRecord =
        serde_json::from_slice(&std::fs::read(dir.join(INPUT_FILE)).unwrap()).unwrap();
    assert_eq!(input.name, "teleop");
    assert_eq!(input.input, json!({"do": "thing"}));
    let output: ToolOutputRecord =
        serde_json::from_slice(&std::fs::read(dir.join(OUTPUT_FILE)).unwrap()).unwrap();
    assert_eq!(output.exit_code, 0);
    assert_eq!(output.stdout, "routed product");
    assert_eq!(output.started_at, "iso-1");
    assert_eq!(output.ended_at, "iso-2");
}

#[test]
fn a_vanished_endpoint_is_an_in_band_error_result() {
    // The obligation the contract states: unreachable is a non-zero
    // result the model reads, never a harness fault and never a hang.
    let root = HarnessRoot::new();
    let clock = FixedClock::default();
    let step = StepDir::new();
    let host = Embedder::failing("teleop");
    let exec = SpawnTool::new(root.path(), &clock, driver_target()).with_injection(Some(&host));
    let outcome = exec
        .execute(
            ToolCall {
                id: "toolu_dead",
                name: "teleop",
                input: &json!({}),
            },
            &step.path,
            &AtomicBool::new(false),
            None,
        )
        .unwrap();
    assert!(
        outcome.is_error,
        "a non-zero routed exit is an error result"
    );
    assert_eq!(
        String::from_utf8_lossy(&outcome.content),
        "Exit code: 7\nrouted product\n--- stderr ---\nendpoint vanished",
    );
}

#[test]
fn the_executor_reports_the_hosts_definitions_and_nothing_without_one() {
    // The declaration half: what the composer splices and what the grant
    // gate unions, read off the executor that will answer them.
    let root = HarnessRoot::new();
    let clock = FixedClock::default();
    let host = Embedder::new("teleop");
    let with = SpawnTool::new(root.path(), &clock, driver_target()).with_injection(Some(&host));
    let names: Vec<String> = with
        .injected(std::path::Path::new("/ws"), "agent-1")
        .into_iter()
        .map(|t| t.name)
        .collect();
    assert_eq!(names, vec!["teleop".to_string()]);
    let without = SpawnTool::new(root.path(), &clock, driver_target());
    assert!(
        without
            .injected(std::path::Path::new("/ws"), "agent-1")
            .is_empty(),
        "no injection installed declares nothing"
    );
}
