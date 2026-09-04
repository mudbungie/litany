//! The learning loop end to end on the stub-adapter harness
//! (`docs/DESIGN_LEARNING_LOOP.md` §6): a **scripted reviewer** emits an
//! `apply_patch` and a final response, its dispatcher's executor stages
//! the proposal, an operator accepts it, and the next election on the
//! lineage sees the patched skill — with no act per agent
//! (follow-the-tip, ARCH §2.2).
//!
//! The verifier gate's own proof pattern ([`super::verifier_gate`]),
//! and for the same reason: every piece is driven through the ordinary
//! `litany advance` hop, so what is proved is the shipped path rather
//! than a rehearsal of it. Two facts ride the **adapter** the parent's
//! hop is given — an [`unreachable_adapter`]: a staged proposal costs
//! the reviewed agent no model call, and its reasoning never enters
//! that agent's context.

use super::advance::{RecLauncher, worker_config};
use super::fixtures::*;
use crate::config::Workflow;
use crate::prompt::child_dispatch::{ChildDispatchRequest, run as dispatch_child};
use crate::prompt::dispatch::advance::{AdvanceOutcome, run};
use crate::prompt::resolve::WorkerConfig;
use crate::prompt::tool::{ExecError, ToolCall, ToolExecutor, ToolOutcome};
use crate::prompt::{Deps, NanoIdGen, SystemClock};
use crate::template::{GitRunner, RealGit};
use crate::workspace::agent_name::mint::test_rng;
use crate::workspace::{agent_worktree, config_ref, fixture, proposal, repo_git};
use brazen::FinishReason;
use serde_json::json;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;

/// The learning loop's binding for a reviewer's return
/// (`workflows/learning-loop.yaml`).
const STAGE: &str = "events:\n  reviewer_return:\n    - stage_proposal\n";

/// A `SKILL.md` the descriptions snapshot's parser accepts.
fn manifest(body: &str) -> String {
    format!("---\nname: notes\ndescription: d\n---\n{body}")
}

/// The tool executor a scripted reviewer runs against: the stub's disk
/// contract, plus the **worktree side effect** a real `apply_patch`
/// would leave. The executor commits the tool entry with `git add -A`
/// (ARCH §2.3), so the edit lands on the reviewer's branch exactly as a
/// real patch's would — which is what makes the staged diff below the
/// reviewer's own work rather than the fixture's.
struct PatchingTools {
    inner: StubToolExecutor,
    worktree: PathBuf,
    body: String,
}

impl ToolExecutor for PatchingTools {
    fn execute(
        &self,
        call: ToolCall<'_>,
        step_dir: &Path,
        stop: &AtomicBool,
        bound: Option<crate::config::ToolOutputBound>,
    ) -> Result<ToolOutcome, ExecError> {
        let outcome = self.inner.execute(call, step_dir, stop, bound)?;
        let dest = self.worktree.join("skills/notes/SKILL.md");
        std::fs::create_dir_all(dest.parent().expect("skills/notes has a parent"))
            .and_then(|()| std::fs::write(&dest, &self.body))
            .expect("the reviewer's tree is writable");
        Ok(outcome)
    }
}

/// The reviewer's two model calls: an `apply_patch` tool use, then the
/// closing response the proposal's commit message is taken from.
fn scripted_reviewer(subject: &str) -> StubAdapter {
    StubAdapter::scripted([
        StubAdapter::reply_ok(&stream_of(
            FinishReason::ToolUse,
            &[Block::ToolUse {
                id: "tu-1",
                name: "apply_patch",
                input: json!({ "patch": "*** Update File: skills/notes/SKILL.md" }),
            }],
        )),
        StubAdapter::reply_ok(&stream_of(FinishReason::Stop, &[Block::Text(subject)])),
    ])
}

/// A [`WorkerConfig`] for one hop: the role, its grant, and the workflow
/// that governs the hop's own bindings.
fn cfg(role: &str, tools: &[&str], workflow: &str) -> WorkerConfig {
    WorkerConfig {
        role: role.into(),
        tools: tools.iter().map(|t| (*t).to_string()).collect(),
        workflow: Workflow::parse(workflow, Path::new("workflow.yaml")).unwrap(),
        ..worker_config()
    }
}

#[test]
fn a_reviewer_proposes_an_operator_accepts_and_the_next_election_sees_it() {
    let (holder, ws) = fixture::workspace();
    let parent = "20260101-a1";
    // The branch forks first and the lineage gains the skill after, so
    // the dispatching branch's tree carries no body of its own —
    // production's dispatch commit trims exactly that
    // (`crate::prompt::dispatch::trim_to_context`), and the election at
    // the end is then a real election rather than an already-loaded
    // copy.
    let parent_wt = fixture::spawn_root(&ws, parent);
    fixture::amend_config(
        &ws,
        &[("skills/notes/SKILL.md", manifest("the lesson").as_str())],
    );

    let git = RealGit::new();
    let clock = SystemClock;
    let id = NanoIdGen;
    let sleeper = StubSleeper::default();
    let stop = AtomicBool::new(false);
    let rec = RecLauncher::default();
    let cfg_root = tempfile::TempDir::new().unwrap();
    let data_root = holder.path().join("data");

    // The checkpoint's fork, without the clock: a reviewer child off the
    // dispatching branch, its dispatch commit carrying the lineage's
    // skills and its read mark (`crate::prompt::dispatch::step_commit`).
    let reviewer = dispatch_child(
        &ChildDispatchRequest {
            repo: &ws,
            parent_branch: parent,
            parent_worktree: &parent_wt,
            role: "reviewer",
            goal: "review the span",
            name: None,
            fork_point: None,
            cwd: None,
            pins: crate::prompt::PinnedDocs::none(),
        },
        &git,
        &clock,
        &id,
        &rec,
        test_rng(),
    )
    .unwrap();

    // (1) The reviewer's own hop: patch, then answer.
    let patched = manifest("the lesson, corrected by review");
    let tools = PatchingTools {
        inner: StubToolExecutor::ok(),
        worktree: agent_worktree(&ws, &reviewer),
        body: patched.clone(),
    };
    let adapter = scripted_reviewer("notes: record what the span taught");
    let deps = Deps {
        adapter: &adapter,
        sleeper: &sleeper,
        git: &git,
        clock: &clock,
        id_gen: &id,
        tool_executor: &tools,
        config_root: cfg_root.path(),
        data_root: &data_root,
        adapter_target: None,
        stop: &stop,
        launcher: &rec,
        rng: test_rng(),
    };
    // Two hops, because a hop is one step (§6): the first patches, the
    // second answers and runs the exit protocol, depositing the result
    // into the dispatcher's inbox (§2.6).
    for _ in 0..2 {
        run(&ws, &reviewer, None, &deps, &mut || {
            Ok(cfg("reviewer", &["apply_patch"], "events: {}\n"))
        })
        .unwrap();
    }

    // (2) The dispatcher's hop stages it — and takes no model call
    // doing so: the adapter here answers nothing at all.
    let quiet = unreachable_adapter();
    let deps = Deps {
        adapter: &quiet,
        tool_executor: &StubToolExecutor::ok(),
        ..deps
    };
    let out = run(&ws, parent, None, &deps, &mut || {
        Ok(cfg("worker", &["bash"], STAGE))
    })
    .unwrap();
    assert!(
        matches!(out, AdvanceOutcome::NothingToDo),
        "a review wakes nobody into a model call: {out:?}"
    );
    assert!(
        !parent_wt.join("messages").exists(),
        "and enters no transcript"
    );

    // The proposal stands, fresh, carrying the reviewer's own text.
    let rows = proposal::list(&ws, &git).unwrap();
    assert_eq!(rows.len(), 1, "one proposal: {rows:?}");
    assert_eq!(rows[0].id, reviewer);
    assert!(rows[0].fresh);
    assert_eq!(rows[0].subject, "notes: record what the span taught");

    // (3) The operator accepts: the lineage fast-forwards onto it.
    proposal::accept(&ws, &reviewer, &git).unwrap();
    let tip = git
        .run_capture(&repo_git(&ws), &["rev-parse", &config_ref("default")])
        .unwrap();
    assert_eq!(
        git.run_capture(
            &repo_git(&ws),
            &["show", &format!("{}:skills/notes/SKILL.md", tip.trim())]
        )
        .unwrap()
        .trim(),
        patched.trim(),
        "the lineage carries the accepted body"
    );

    // (4) No act per agent: the dispatching branch, untouched since
    // before the review, elects the skill and gets the patched body —
    // follow-the-tip resolves the accepted commit at its next step.
    let mut out = Vec::new();
    crate::prompt::tool::builtin::load_skill::run(
        &mut std::io::Cursor::new(json!({"name": "notes"}).to_string().into_bytes()),
        &mut out,
        &election_env(&ws, parent),
    )
    .unwrap();
    assert_eq!(
        std::fs::read_to_string(parent_wt.join("skills/notes/SKILL.md")).unwrap(),
        patched,
        "the next election reads the accepted lesson"
    );
}

/// The tool contract's three environment facts an election reads
/// (ARCH §3.3): which workspace, which branch, and the harness root the
/// install pool hangs off. The pool is deliberately somewhere with no
/// skills — the body under test is the *workspace's*, resolved from the
/// followed config commit first (§3 there).
struct ElectionEnv(std::collections::HashMap<&'static str, std::ffi::OsString>);

impl crate::prompt::tool::builtin::dispatch::EnvLookup for ElectionEnv {
    fn get(&self, key: &str) -> Option<std::ffi::OsString> {
        self.0.get(key).cloned()
    }
}

fn election_env(ws: &Path, agent: &str) -> ElectionEnv {
    ElectionEnv(std::collections::HashMap::from([
        ("LITANY_CONV_REPO", ws.as_os_str().to_owned()),
        ("LITANY_CONV_BRANCH", std::ffi::OsString::from(agent)),
        ("LITANY_HOME", ws.as_os_str().to_owned()),
    ]))
}
