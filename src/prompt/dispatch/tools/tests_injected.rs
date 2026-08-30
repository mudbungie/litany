//! Injection at the composer (ARCH §3.3 *Host-injected tools*, §2.7,
//! `docs/DESIGN_TOOL_INJECTION.md`).
//!
//! Two sources — the calling role's procedure and the binding's host
//! injection — meet in [`super::injected`] and are spliced by
//! [`super::compose`] as one list, ahead of election. Split into its own
//! file so [`super::tests`] stays under the 300-line cap.

use super::tests::{BASH_SCHEMA, custom, history_calling, write_schema};
use super::*;
use crate::prompt::compactor::COMPACTOR_ROLE;
use crate::prompt::tool::inject::{InjectedTool, RoutedCall, RoutedCapture, ToolInjection};
use crate::prompt::tool::{ExecError, SpawnTool, ToolCall, ToolOutcome};
use serde_json::json;
use std::path::Path;
use tempfile::TempDir;

/// A test embedder declaring one tool. It never routes anything: this
/// file's subject is the declaration half.
struct Host(&'static str);

impl ToolInjection for Host {
    fn tools(&self) -> Vec<InjectedTool> {
        vec![InjectedTool {
            name: self.0.to_string(),
            input_schema: json!({"type": "object", "properties": {"q": {"type": "string"}}}),
            description: Some("the host's own tool".into()),
        }]
    }

    fn route(&self, _call: RoutedCall<'_>) -> RoutedCapture {
        unreachable!("this file's subject is the declaration half")
    }
}

/// An executor that carries no injection of its own — the trait's
/// default `injected`, which is what every in-process stub inherits.
struct Bare;

impl ToolExecutor for Bare {
    fn execute(
        &self,
        _call: ToolCall<'_>,
        _step_dir: &Path,
        _stop: &std::sync::atomic::AtomicBool,
        _bound: Option<crate::config::ToolOutputBound>,
    ) -> Result<ToolOutcome, ExecError> {
        unreachable!("nothing executes in the composer's tests")
    }
}

/// The production executor with a host installed — the same object the
/// bindings build, so what the composer reads here is what a real drive
/// would read.
fn hosted<'a>(
    host: &'a Host,
    root: &'a Path,
    clock: &'a dyn crate::prompt::Clock,
) -> SpawnTool<'a> {
    SpawnTool::new(root, clock, Path::new("litany")).with_injection(Some(host))
}

#[test]
fn the_two_injection_sources_meet_in_one_list() {
    // The compactor's procedure pair (§2.7) and the host's tool (§3.3):
    // one list, procedure first, read by the composer and the grant gate
    // alike.
    let host = Host("teleop");
    let root = TempDir::new().unwrap();
    let clock = crate::prompt::clock::SystemClock;
    let exec = hosted(&host, root.path(), &clock);
    let names: Vec<String> = injected(COMPACTOR_ROLE, &exec)
        .into_iter()
        .map(|t| t.name)
        .collect();
    assert_eq!(names, vec!["write_summary", "mark_for_deletion", "teleop"]);

    // An ordinary role gets the host's alone; without a host, nothing.
    let worker: Vec<String> = injected(crate::prompt::WORKER_ROLE, &exec)
        .into_iter()
        .map(|t| t.name)
        .collect();
    assert_eq!(worker, vec!["teleop"]);
    assert!(injected(crate::prompt::WORKER_ROLE, &Bare).is_empty());
}

#[test]
fn an_injected_definition_composes_verbatim_and_ahead_of_election() {
    // The host's three facts ride the wire as the entry's three facts,
    // and injection is spliced before the role's elected tools.
    let wt = TempDir::new().unwrap();
    write_schema(wt.path(), "bash", BASH_SCHEMA);
    let host = Host("teleop");
    let tools = compose(wt.path(), &["bash".to_string()], &[], &host.tools()).unwrap();
    let names: Vec<&str> = tools.iter().map(|t| custom(t).0).collect();
    assert_eq!(names, vec!["teleop", "bash"]);
    let (_, description, schema) = custom(&tools[0]);
    assert_eq!(description, Some("the host's own tool"));
    assert_eq!(*schema, host.tools()[0].input_schema);
}

#[test]
fn an_injected_name_outranks_the_elected_tool_it_shadows() {
    // One name, one entry, and it is the injected one — because the
    // router also answers it first, so the model must read the schema of
    // what will actually run.
    let wt = TempDir::new().unwrap();
    write_schema(wt.path(), "bash", BASH_SCHEMA);
    let host = Host("bash");
    let tools = compose(wt.path(), &["bash".to_string()], &[], &host.tools()).unwrap();
    let names: Vec<&str> = tools.iter().map(|t| custom(t).0).collect();
    assert_eq!(names, vec!["bash"], "declared once, never twice");
    assert_eq!(custom(&tools[0]).1, Some("the host's own tool"));
}

#[test]
fn an_injected_name_is_never_re_declared_by_the_history_closure() {
    // The closure appends what the history names and the array lacks
    // (§3.3); an injected name is already there.
    let wt = TempDir::new().unwrap();
    let host = Host("teleop");
    let tools = compose(
        wt.path(),
        &[],
        &history_calling(&["teleop", "frobnicate"]),
        &host.tools(),
    )
    .unwrap();
    let names: Vec<&str> = tools.iter().map(|t| custom(t).0).collect();
    assert_eq!(names, vec!["teleop", "frobnicate"]);
}
