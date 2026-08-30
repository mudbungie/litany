//! §6 live-compaction / role-aware wiring on the `litany advance` hop:
//! the delivered-child-result interpretation reached through `run`, and
//! the compactor-role built-in-toolset injection. Split out of
//! [`super::advance`] so that file stays under the per-file line cap; the
//! shared helpers (`worker_config`, `AGENT`, `RecLauncher`, …) live there.

use super::advance::{
    AGENT, RecLauncher, model_entry, terminal_tail, worker_config, workspace_with_tail,
};
use super::fixtures::*;
use crate::prompt::Clock;
use crate::prompt::dispatch::advance::{AdvanceOutcome, run};
use crate::prompt::inbox;
use crate::prompt::resolve::WorkerConfig;
use crate::workspace::agent_name::mint::test_rng;

/// A hyphen-free compact stamp (§2.3 — "both the compact timestamp and
/// the short id are hyphen-free"), so a dispatched child's id is a clean
/// two-token descent segment and `inbox::parent_of` derives the
/// dispatcher its result message must reach (§2.6).
struct DescentClock;
impl Clock for DescentClock {
    fn now_iso8601(&self) -> String {
        "iso".into()
    }
    fn now_compact(&self) -> String {
        "ct1".into()
    }
}

/// A [`worker_config`] specialized to the compactor role — the shape a
/// dispatched compactor resolves (§6). Drives the step's built-in-toolset
/// injection (§2.7).
fn compactor_config() -> WorkerConfig {
    WorkerConfig {
        role: "compactor".into(),
        // The shipped compactor row grants no tools (§4.3): its toolset
        // is the injected pair alone (§2.7).
        tools: vec![],
        ..worker_config()
    }
}

#[test]
fn a_pending_worker_result_is_interpreted_then_the_branch_steps() {
    // §6 end-to-end wiring on the hop: a worker child's result message,
    // left in the inbox by the drain, is interpreted (deliver_result:
    // transfer + transcript delivery), which makes the tail user-side, and
    // the branch steps to react — reusing the config resolved for the
    // interpretation. Real git (transfer needs it); stub adapter/tools.
    use crate::prompt::child_dispatch::{ChildDispatchRequest, run as dispatch_child};
    use crate::prompt::inbox::deposit_result;
    use crate::template::{GitRunner, RealGit};
    use crate::workspace::{agent_worktree, fixture};

    let (_h, ws) = fixture::workspace();
    let parent = AGENT;
    let parent_wt = fixture::spawn_root(&ws, parent);
    let git = RealGit::new();
    let (clock, id) = (FixedClock::default(), FixedIdGen);
    let rec = RecLauncher::default();
    // Fork a worker child, commit a work product, deposit its result.
    let req = ChildDispatchRequest {
        repo: &ws,
        parent_branch: parent,
        parent_worktree: &parent_wt,
        role: "worker",
        goal: "do it",
        name: None,
        fork_point: None,
        cwd: None,
        pins: crate::prompt::PinnedDocs::none(),
    };
    let child = dispatch_child(&req, &git, &DescentClock, &id, &rec, test_rng()).unwrap();
    let child_wt = agent_worktree(&ws, &child);
    std::fs::write(child_wt.join("out.txt"), "result\n").unwrap();
    git.run(&child_wt, &["add", "-A"]).unwrap();
    git.run(&child_wt, &["commit", "-m", "work"]).unwrap();
    let tip = git.run_capture(&child_wt, &["rev-parse", "HEAD"]).unwrap();
    deposit_result(
        &ws,
        parent,
        &child,
        inbox::Epitaph::FinalResponse,
        tip.trim(),
        Some("done"),
        &clock,
        &git,
    )
    .unwrap();

    let adapter = StubAdapter::scripted([StubAdapter::reply_ok(&happy_response_bytes())]);
    let (sleeper, tools, stub_git) = (
        StubSleeper::default(),
        StubToolExecutor::ok(),
        StubGit::ok(),
    );
    let mut deps = valid_deps(&adapter, &sleeper, &stub_git, &clock, &id, &tools, &ws);
    deps.git = &git;
    deps.launcher = &rec;
    let out = run(&ws, parent, None, &deps, &mut || Ok(worker_config())).unwrap();

    assert!(matches!(out, AdvanceOutcome::Terminal));
    // The child's work product transferred into the parent tree, and its
    // result message delivered to the transcript, then a step answered
    // with a final response — asserted on the disk record (the step's
    // committed response), not a carried payload.
    assert_eq!(
        std::fs::read_to_string(parent_wt.join("out.txt")).unwrap(),
        "result\n"
    );
    assert!(parent_wt.join(format!("messages/001-{child}.md")).exists());
    assert!(
        ws.join(format!("steps/{parent}/001/response.json"))
            .exists()
    );
}

#[test]
fn a_compaction_landing_lands_the_product_and_the_next_step_assembles_clean() {
    // §2.6/§2.7 compaction landing (rebase-forward), end to end on the hop. A
    // compactor is an ordinary child, so its branch carries its own
    // dispatch `goal.md`/`soul.md` and its own transcript — ending, here,
    // on a failed tool entry whose `tool_result` has no `tool_use`
    // anywhere on the dispatching branch. Only the summary and the
    // nominated deletion cross; the parent's next step then assembles a
    // clean history instead of wedging on the imported entries.
    use crate::prompt::child_dispatch::{ChildDispatchRequest, run as dispatch_child};
    use crate::prompt::inbox::deposit_result;
    use crate::template::{GitRunner, RealGit};
    use crate::workspace::{agent_worktree, fixture};
    use brazen::Content;

    let (_h, ws) = fixture::workspace();
    let parent = AGENT;
    let parent_wt = fixture::spawn_root(&ws, parent);
    let git = RealGit::new();
    let (clock, id) = (FixedClock::default(), FixedIdGen);
    let rec = RecLauncher::default();

    // The dispatching branch's transcript at the checkpoint commit C.
    std::fs::create_dir_all(parent_wt.join("messages")).unwrap();
    std::fs::write(parent_wt.join("messages/001-user.md"), "old\n").unwrap();
    git.run(&parent_wt, &["add", "-A"]).unwrap();
    git.run(&parent_wt, &["commit", "-m", "checkpoint"])
        .unwrap();

    // A compactor forked off C: summary written, superseded entry
    // nominated, and its own dialog accumulated alongside.
    let req = ChildDispatchRequest {
        repo: &ws,
        parent_branch: parent,
        parent_worktree: &parent_wt,
        role: "compactor",
        goal: "compact",
        name: None,
        fork_point: None,
        cwd: None,
        pins: crate::prompt::PinnedDocs::none(),
    };
    let child = dispatch_child(&req, &git, &DescentClock, &id, &rec, test_rng()).unwrap();
    let cwt = agent_worktree(&ws, &child);
    std::fs::create_dir_all(cwt.join("summary")).unwrap();
    std::fs::write(cwt.join("summary/001.md"), "digest\n").unwrap();
    let dialog = [
        ("002-goal.md", "compact the branch".to_string()),
        (
            // The compactor's own model (§4.3 role config), so the name
            // cannot be confused with the parent's own step output.
            "003-claude-haiku-5.json",
            model_entry(&[Content::Text("looking".into())]),
        ),
        (
            "004-tool.json",
            serde_json::to_string(&[Content::ToolResult {
                tool_use_id: "toolu_ghost".into(),
                content: vec![Content::Text("no such path".into())],
                is_error: true,
            }])
            .unwrap(),
        ),
    ];
    for (name, body) in &dialog {
        std::fs::write(cwt.join("messages").join(name), body).unwrap();
    }
    git.run(&cwt, &["rm", "-q", "--", "messages/001-user.md"])
        .unwrap();
    git.run(&cwt, &["add", "-A"]).unwrap();
    git.run(&cwt, &["commit", "-m", "compaction"]).unwrap();
    let tip = git.run_capture(&cwt, &["rev-parse", "HEAD"]).unwrap();
    deposit_result(
        &ws,
        parent,
        &child,
        inbox::Epitaph::FinalResponse,
        tip.trim(),
        Some("compacted"),
        &clock,
        &git,
    )
    .unwrap();
    // A steering deposit warrants the parent's next step (§2.3).
    inbox::deposit(&ws, parent, "user", "carry on", &clock).unwrap();

    let adapter = StubAdapter::scripted([StubAdapter::reply_ok(&happy_response_bytes())]);
    let (sleeper, tools, stub_git) = (
        StubSleeper::default(),
        StubToolExecutor::ok(),
        StubGit::ok(),
    );
    let mut deps = valid_deps(&adapter, &sleeper, &stub_git, &clock, &id, &tools, &ws);
    deps.git = &git;
    deps.launcher = &rec;
    let out = run(&ws, parent, None, &deps, &mut || Ok(worker_config())).unwrap();

    assert!(matches!(out, AdvanceOutcome::Terminal), "the step ran");
    // The compaction product landed: summary in, superseded entry out.
    assert_eq!(
        std::fs::read_to_string(parent_wt.join("summary/001.md")).unwrap(),
        "digest\n"
    );
    assert!(!parent_wt.join("messages/001-user.md").exists());
    // Zero compactor transcript entries, and the parent's own goal stands.
    for (name, _) in &dialog {
        assert!(
            !parent_wt.join("messages").join(name).exists(),
            "compactor entry {name} crossed the landing"
        );
    }
    assert!(
        !std::fs::read_to_string(parent_wt.join("goal.md"))
            .unwrap()
            .contains("compact the branch")
    );
    // The compactor's record is its own ref (§2.6).
    assert!(
        git.run_capture(
            &parent_wt,
            &[
                "cat-file",
                "-e",
                &format!("agents/{child}:messages/004-tool.json")
            ]
        )
        .is_ok()
    );
    // The next step assembled clean: no orphaned tool_result reached the
    // wire history the parent sent.
    let req: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(ws.join(format!("steps/{parent}/001/request.json"))).unwrap(),
    )
    .unwrap();
    assert!(
        !req["messages"].to_string().contains("toolu_ghost"),
        "{}",
        req["messages"]
    );
}

#[test]
fn a_compactor_hop_injects_the_builtin_toolset_into_the_request() {
    // §2.7/§6 role-aware resolution: a compactor-role hop composes the
    // built-in write_summary / mark_for_deletion schemas into the request,
    // even though no `descriptions/**` or `providers.yaml` list carries
    // them. Asserted on the step's request.json (written before the call).
    let (ws, _wt) = workspace_with_tail(&terminal_tail());
    let (clock, id) = (FixedClock::default(), FixedIdGen);
    inbox::deposit(ws.path(), AGENT, "user", "compact", &clock).unwrap();
    let adapter = StubAdapter::scripted([StubAdapter::reply_ok(&happy_response_bytes())]);
    let (sleeper, git) = (StubSleeper::default(), StubGit::ok());
    let tools = StubToolExecutor::ok();
    let rec = RecLauncher::default();
    let mut deps = valid_deps(&adapter, &sleeper, &git, &clock, &id, &tools, ws.path());
    deps.launcher = &rec;
    run(
        ws.path(),
        AGENT,
        None,
        &deps,
        &mut || Ok(compactor_config()),
    )
    .unwrap();

    let req: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(ws.path().join(format!("steps/{AGENT}/001/request.json")))
            .unwrap(),
    )
    .unwrap();
    let names: Vec<&str> = req["tools"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["name"].as_str().unwrap())
        .collect();
    assert!(names.contains(&"write_summary"), "{names:?}");
    assert!(names.contains(&"mark_for_deletion"), "{names:?}");
}
