//! The in-process root loop resolves config at **every step boundary**,
//! not once per exchange (bl-e580; ARCH §2.2 follow-the-tip, §6 the
//! workflow mark).
//!
//! The 2026-09-01 ruling is "configuration should be changeable at any
//! time, on any turn". A root's first exchange is an unbounded number of
//! steps long and used to run entirely on the resolution taken before its
//! branch existed, so an edit landing during it reached the agent only
//! once some later `litany advance` hop drove it. Here the edit lands
//! *between* step 1 and step 2 — the tool call is the wall-clock window an
//! operator would type it in — and step 2 must be governed by it.
//!
//! `StubGit`'s `show` serves the workspace's own files as the config
//! commit's tree ([`super::fixtures::scaffold_repo`]), so rewriting them
//! mid-run is a `litany config` commit as this harness models one.

use super::fixtures::*;
use crate::config::ToolOutputBound;
use crate::prompt::ExecError;
use crate::prompt::run;
use crate::prompt::tool::{ToolCall, ToolExecutor, ToolOutcome};
use brazen::FinishReason;
use serde_json::{Value, json};
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;

/// The edited config: a different soul (the §5.2 system slot) and a
/// different model assignment (§4.3) — two facts a step reads from the
/// config commit, one edit.
const EDITED_SOUL: &str = "the edited soul";
const EDITED_PROVIDERS: &str = r#"
roles:
  worker:
    provider: anthropic
    model: claude-opus-5
    tools: [bash, read_file]
"#;

/// A tool executor that performs the operator's edit while the exchange
/// is running, then behaves exactly like [`StubToolExecutor`].
struct EditsConfigMidExchange {
    inner: StubToolExecutor,
    workspace: PathBuf,
}

impl ToolExecutor for EditsConfigMidExchange {
    fn execute(
        &self,
        call: ToolCall<'_>,
        step_dir: &Path,
        stop: &AtomicBool,
        bound: Option<ToolOutputBound>,
    ) -> Result<ToolOutcome, ExecError> {
        std::fs::write(self.workspace.join("souls/worker.md"), EDITED_SOUL).unwrap();
        std::fs::write(self.workspace.join("providers.yaml"), EDITED_PROVIDERS).unwrap();
        self.inner.execute(call, step_dir, stop, bound)
    }
}

/// The request's system slot as one string (§5.2 — one `Content::Text`).
fn system_text(request: &Value) -> String {
    request["system"][0]["text"].as_str().unwrap().to_string()
}

fn request(repo: &Path, step: &str) -> Value {
    let path = repo.join(format!("steps/ct-1-deadbeef/{step}/request.json"));
    serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap()
}

#[test]
fn a_config_edit_between_two_steps_governs_the_second_one() {
    let repo = scaffold_repo(VALID_PER_REPO_PROVIDERS_YAML, Some("the original soul"));
    let harness = scaffold_harness_root();
    let tool_use = stream_of(
        FinishReason::ToolUse,
        &[Block::ToolUse {
            id: "toolu_01",
            name: "bash",
            input: json!({ "cmd": "ls" }),
        }],
    );
    let done = stream_of(FinishReason::Stop, &[Block::Text("done")]);
    // One version guard per boundary: the load-time §4.4 guard is part of
    // resolution, so it runs again with it.
    let adapter = StubAdapter::scripted([
        StubAdapter::reply_ok(&version_line()),
        StubAdapter::reply_ok(&tool_use),
        StubAdapter::reply_ok(&version_line()),
        StubAdapter::reply_ok(&done),
    ]);
    let git = StubGit::ok();
    let (clock, id) = (FixedClock::default(), FixedIdGen);
    let sleeper = StubSleeper::default();
    let unused = StubToolExecutor::ok();
    let editor = EditsConfigMidExchange {
        inner: StubToolExecutor::with_reply("bash", "files: a b"),
        workspace: repo.path().to_path_buf(),
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
    deps.tool_executor = &editor;

    run(
        repo.path(),
        "list files",
        None,
        None,
        None,
        crate::prompt::PinnedDocs::none(),
        None,
        &deps,
    )
    .unwrap();

    // Step 1 ran on the config the start resolved — the fork's answer,
    // taken moments before the branch existed, which is step 1's own
    // boundary.
    let first = request(repo.path(), "001");
    assert_eq!(first["model"], "claude-sonnet-5");
    assert!(
        system_text(&first).contains("the original soul"),
        "{first:#}"
    );

    // Step 2 ran on the edit: a new model assignment and a new soul,
    // with no re-fork, no retarget and no restart (§2.2, bl-403b).
    let second = request(repo.path(), "002");
    assert_eq!(second["model"], "claude-opus-5");
    let system = system_text(&second);
    assert!(system.contains(EDITED_SOUL), "{second:#}");
    assert!(!system.contains("the original soul"), "{second:#}");

    // The transcript names the model that actually authored each entry
    // (§2.3), so the switch is legible on the branch, not only in the
    // step record.
    let worktree = worktree_path(repo.path());
    assert!(worktree.join("messages/002-claude-sonnet-5.json").is_file());
    assert!(worktree.join("messages/004-claude-opus-5.json").is_file());
}
