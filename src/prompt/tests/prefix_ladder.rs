//! The §5.5 measurement (bl-b902): over consecutive steps of one agent,
//! the bytes shared with the previous request — in cache order, tools
//! then system then messages — never shrink except at a declared
//! boundary, and a subtraction from the declared surface is held until
//! one.
//!
//! Four steps. The host injects two tools; after step 2 it retires one
//! (yog's `clients unload` shape), which on the old tree re-cut the
//! array at step 3 and re-billed the whole context. The soul is edited
//! during step 3's tool window — an operator's config edit, a paid miss
//! — so step 4 is the boundary the retirement lands at.

use super::fixtures::*;
use super::tool_stub::StubToolExecutor;
use crate::config::ToolOutputBound;
use crate::prompt::ExecError;
use crate::prompt::run;
use crate::prompt::tool::inject::InjectedTool;
use crate::prompt::tool::{ToolCall, ToolExecutor, ToolOutcome};
use brazen::FinishReason;
use serde_json::{Value, json};
use std::cell::Cell;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;

/// A worker granted nothing from the pool (§4.3): every tool it can
/// reach is the host's.
const HOST_ONLY_PROVIDERS_YAML: &str = r#"
roles:
  worker:
    provider: anthropic
    model: claude-sonnet-5
    tools: []
"#;

/// The host: two tools until two steps have run their tools, then one;
/// and the operator's soul edit inside the third step's tool window.
struct Host {
    inner: StubToolExecutor,
    workspace: PathBuf,
    steps_run: Cell<u32>,
}

fn injected(name: &str) -> InjectedTool {
    InjectedTool {
        name: name.into(),
        input_schema: json!({"type": "object"}),
        description: Some(format!("the host's {name}")),
    }
}

impl ToolExecutor for Host {
    fn execute(
        &self,
        call: ToolCall<'_>,
        step_dir: &Path,
        stop: &AtomicBool,
        bound: Option<ToolOutputBound>,
    ) -> Result<ToolOutcome, ExecError> {
        self.steps_run.set(self.steps_run.get() + 1);
        if self.steps_run.get() == 3 {
            std::fs::write(self.workspace.join("souls/worker.md"), "the edited soul").unwrap();
        }
        self.inner.execute(call, step_dir, stop, bound)
    }

    fn injected(&self, _workspace: &Path, _agent: &str) -> Vec<InjectedTool> {
        if self.steps_run.get() < 2 {
            vec![injected("alpha"), injected("beta")]
        } else {
            vec![injected("alpha")]
        }
    }
}

fn request(repo: &Path, step: u32) -> Value {
    let path = repo.join(format!("steps/ct-1-deadbeef/{step:03}/request.json"));
    serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap()
}

/// The request in provider cache order (`docs/TAXONOMY.md` §8: tools,
/// system, messages), one part per line so an appended message extends
/// the bytes rather than re-closing an array around them.
fn cache_order(request: &Value) -> Vec<u8> {
    let mut out = format!("{}\n{}\n", request["tools"], request["system"]).into_bytes();
    for message in request["messages"].as_array().unwrap() {
        out.extend(format!("{message}\n").into_bytes());
    }
    out
}

fn shared_prefix(a: &[u8], b: &[u8]) -> usize {
    a.iter().zip(b).take_while(|(x, y)| x == y).count()
}

fn tool_names(request: &Value) -> Vec<String> {
    request["tools"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["name"].as_str().unwrap().to_owned())
        .collect()
}

#[test]
fn the_shared_prefix_is_monotone_except_at_a_declared_boundary() {
    // No pool grant: the declared surface is the host's alone, so the
    // ladder measures injection, not the stub git's descriptor cut.
    let repo = scaffold_repo(HOST_ONLY_PROVIDERS_YAML, Some("the original soul"));
    let harness = scaffold_harness_root();
    let tool_use = stream_of(
        FinishReason::ToolUse,
        &[Block::ToolUse {
            id: "toolu_01",
            name: "alpha",
            input: json!({ "cmd": "ls" }),
        }],
    );
    let done = stream_of(FinishReason::Stop, &[Block::Text("done")]);
    let adapter = StubAdapter::scripted([
        StubAdapter::reply_ok(&version_line()),
        StubAdapter::reply_ok(&tool_use),
        StubAdapter::reply_ok(&version_line()),
        StubAdapter::reply_ok(&tool_use),
        StubAdapter::reply_ok(&version_line()),
        StubAdapter::reply_ok(&tool_use),
        StubAdapter::reply_ok(&version_line()),
        StubAdapter::reply_ok(&done),
    ]);
    let git = StubGit::ok();
    let (clock, id) = (FixedClock::default(), FixedIdGen);
    let sleeper = StubSleeper::default();
    let unused = StubToolExecutor::ok();
    let host = Host {
        inner: StubToolExecutor::with_reply("alpha", "files: a b"),
        workspace: repo.path().to_path_buf(),
        steps_run: Cell::new(0),
    };
    let mut deps = valid_deps(
        &adapter,
        &sleeper,
        &git,
        &clock,
        &id,
        &unused,
        harness.path(),
    );
    deps.tool_executor = &host;

    run(
        repo.path(),
        "list files",
        None,
        None,
        None,
        crate::prompt::PinnedDocs::none(),
        None,
        None,
        &deps,
    )
    .unwrap();

    let steps: Vec<Value> = (1..=4).map(|s| request(repo.path(), s)).collect();
    let bytes: Vec<Vec<u8>> = steps.iter().map(cache_order).collect();

    // Steps 1→2 and 2→3: the previous request is a byte prefix of the
    // next, whole — step 3 included, though the host had retired `beta`
    // by then: the subtraction is held.
    for k in 0..2 {
        assert_eq!(
            shared_prefix(&bytes[k], &bytes[k + 1]),
            bytes[k].len(),
            "step {} is not a byte prefix of step {}",
            k + 1,
            k + 2
        );
    }
    assert!(
        tool_names(&steps[2]).contains(&"beta".to_owned()),
        "{:?}",
        tool_names(&steps[2])
    );

    // Step 4 is the declared boundary — the soul moved, so the miss is
    // paid — and the retirement rides it: the shared prefix drops below
    // the previous run's, and `beta` is gone.
    let at_boundary = shared_prefix(&bytes[2], &bytes[3]);
    assert!(
        at_boundary < shared_prefix(&bytes[1], &bytes[2]),
        "{at_boundary}"
    );
    assert!(
        !tool_names(&steps[3]).contains(&"beta".to_owned()),
        "{:?}",
        tool_names(&steps[3])
    );
    assert!(
        steps[3]["system"][0]["text"]
            .as_str()
            .unwrap()
            .contains("the edited soul")
    );
}
