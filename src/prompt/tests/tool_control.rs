//! The tool-control seam through both drivers (ARCH §3.3 *Tool
//! control*, §6): a fresh hold parks `run_exchange` and the `litany
//! advance` hop without a terminal. The parked-branch lifecycle —
//! queued mail, resume, stale sweep — is [`super::tool_control_resume`]
//! (split to hold the 300-line code-file cap).

use super::advance::{AGENT, RecLauncher, eventually_free, worker_config};
use super::fixtures::*;
use crate::config::Workflow;
use crate::prompt::dispatch::advance::{AdvanceOutcome, run};
use crate::prompt::inbox::{self, inbox_dir, try_acquire};
use crate::prompt::resolve::WorkerConfig;
use crate::prompt::run as prompt_run;
use crate::workspace::agent_name::mint::test_rng;
use brazen::FinishReason;
use serde_json::json;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use tempfile::TempDir;

/// An executable fixture control that holds until `<workspace>/approval`
/// exists, then passes — the release living entirely in the control's
/// own out-of-band fact, as §3.3 specifies.
pub(super) fn approval_control(dir: &Path) -> PathBuf {
    let path = dir.join("fixture-control.sh");
    std::fs::write(
        &path,
        "#!/bin/sh\n\
         if [ -f \"$LITANY_CONV_REPO/approval\" ]; then \
           echo '{\"verdict\":\"pass\"}'; \
         else \
           echo '{\"verdict\":\"hold\",\"reason\":\"awaiting approval\"}'; \
         fi\n",
    )
    .unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
    path
}

pub(super) fn gated_workflow(control: &Path) -> String {
    format!(
        "events: {{}}\ntool_control:\n  command: {}\n",
        control.display()
    )
}

/// A [`crate::prompt::Deps`] over real git — the marks need a real ref
/// store ([`fixtures::valid_deps`] is `StubGit`-typed). Config never
/// resolves from disk here (resolution is injected), so any path serves
/// as `config_root`.
pub(super) fn real_deps<'a>(
    adapter: &'a StubAdapter,
    sleeper: &'a StubSleeper,
    git: &'a dyn crate::template::GitRunner,
    clock: &'a FixedClock,
    id: &'a FixedIdGen,
    tools: &'a StubToolExecutor,
    config_root: &'a Path,
) -> crate::prompt::Deps<'a> {
    crate::prompt::Deps {
        adapter,
        sleeper,
        git,
        clock,
        id_gen: id,
        tool_executor: tools,
        config_root,
        adapter_target: None,
        stop: never_stopped(),
        launcher: no_launch(),
        rng: test_rng(),
    }
}

#[test]
fn a_hold_parks_run_exchange_without_a_terminal() {
    // The in-process root driver: the control holds the first tool_use,
    // so the loop ceases mid-window — no executor entry, no tool_result,
    // no terminal deposit — and the lease releases for a later advance.
    let scripts = TempDir::new().unwrap();
    let control = approval_control(scripts.path());
    let repo = scaffold_repo_with_workflow(
        VALID_PER_REPO_PROVIDERS_YAML,
        &gated_workflow(&control),
        Some("body"),
    );
    let harness = scaffold_harness_root();
    let tool_stream = stream_of(
        FinishReason::ToolUse,
        &[Block::ToolUse {
            id: "toolu_01",
            name: "bash",
            input: json!({"command": "true"}),
        }],
    );
    let adapter = StubAdapter::happy(&tool_stream);
    let git = StubGit::ok();
    let (clock, id) = (FixedClock::default(), FixedIdGen);
    let (sleeper, tool_executor) = (StubSleeper::default(), StubToolExecutor::ok());
    let branch = prompt_run(
        repo.path(),
        "do the thing",
        None,
        None,
        None,
        crate::prompt::PinnedDocs::none(),
        None,
        &valid_deps(
            &adapter,
            &sleeper,
            &git,
            &clock,
            &id,
            &tool_executor,
            harness.path(),
        ),
    )
    .unwrap();
    assert_eq!(branch, "ct-1-deadbeef");
    // The executor was never entered and no tool result committed.
    assert!(tool_executor.invocations.borrow().is_empty());
    let worktree = worktree_path(repo.path());
    assert!(worktree.join("messages/002-claude-sonnet-5.json").exists());
    assert!(!worktree.join("messages/003-tool.json").exists());
    // The park wrote the hold mark (StubGit records the ref write).
    let runs = git.runs.borrow();
    assert!(
        runs.iter().any(
            |(_, args)| args.first().map(String::as_str) == Some("update-ref")
                && args.iter().any(|a| a == "refs/litany/held/ct-1-deadbeef")
        ),
        "no hold-mark write recorded"
    );
    // No terminal: exactly one model call followed the version guard.
    assert_eq!(adapter.observed.borrow().len(), 2);
    // The lease released on the way out.
    assert!(
        try_acquire(&inbox_dir(repo.path(), "ct-1-deadbeef"))
            .unwrap()
            .is_some()
    );
}

#[test]
fn a_fresh_hold_in_a_hop_exits_held_with_the_lease_released() {
    // The advance driver, same circumstance: deposit → step emits
    // tool_use → control holds → AdvanceOutcome::Held, no terminal, no
    // launch, lease free.
    let scripts = TempDir::new().unwrap();
    let control = approval_control(scripts.path());
    let (ws, wt) = super::advance::workspace_with_tail(&super::advance::terminal_tail());
    // The mark's staging home (§2.2 `repo.git`): a directory is enough
    // under StubGit, which records the ref write without a real store.
    std::fs::create_dir(ws.path().join(crate::workspace::REPO_DIR)).unwrap();
    let (clock, id) = (FixedClock::default(), FixedIdGen);
    inbox::deposit(ws.path(), AGENT, "user", "run it", &clock).unwrap();
    let tool_stream = stream_of(
        FinishReason::ToolUse,
        &[Block::ToolUse {
            id: "t1",
            name: "bash",
            input: json!({"command": "true"}),
        }],
    );
    let adapter = StubAdapter::scripted([StubAdapter::reply_ok(&tool_stream)]);
    let (sleeper, git) = (StubSleeper::default(), StubGit::ok());
    let tools = StubToolExecutor::ok();
    let rec = RecLauncher::default();
    let mut deps = valid_deps(&adapter, &sleeper, &git, &clock, &id, &tools, ws.path());
    deps.launcher = &rec;
    let mut cfg = || -> Result<WorkerConfig, crate::prompt::Error> {
        Ok(WorkerConfig {
            workflow: Workflow::parse(&gated_workflow(&control), Path::new("workflow.yaml"))
                .unwrap(),
            ..worker_config()
        })
    };
    let out = run(ws.path(), AGENT, None, &deps, &mut cfg).unwrap();
    assert!(matches!(out, AdvanceOutcome::Held), "got {out:?}");
    assert!(tools.invocations.borrow().is_empty());
    assert!(!wt.join("messages/005-tool.json").exists());
    assert!(
        rec.invocations.borrow().is_empty(),
        "a park launches nothing"
    );
    assert!(eventually_free(ws.path(), AGENT));
}
