//! The parked-branch lifecycle through `litany advance` (ARCH §3.3
//! *Tool control*): a parked branch queues its mail and resumes by
//! fresh adjudication — skip, lift, hand off — plus the stale-mark
//! sweep, the missing-worktree total path, and a stop mid-resume.

use super::advance::{AGENT, RecLauncher, eventually_free, worker_config};
use super::fixtures::*;
use super::tool_control::{approval_control, gated_workflow, real_deps};
use crate::config::Workflow;
use crate::prompt::dispatch::advance::{AdvanceOutcome, run};
use crate::prompt::inbox;
use crate::prompt::resolve::WorkerConfig;
use crate::template::{GitRunner, RealGit};
use crate::workspace::hold;
use brazen::Content;
use serde_json::json;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use tempfile::TempDir;

/// A real-git parked workspace: bare `repo.git` for the marks, a real
/// worktree repo on the agent branch carrying `entries`, and
/// `steps/<AGENT>/001`. Returns the worktree.
fn parked_workspace(ws: &Path, entries: &[(&str, String)]) -> PathBuf {
    let git = RealGit::new();
    git.run(ws, &["init", "--bare", "repo.git"]).unwrap();
    let wt = crate::workspace::agent_worktree(ws, AGENT);
    std::fs::create_dir_all(&wt).unwrap();
    let branch = crate::workspace::agent_ref(AGENT);
    git.run(&wt, &["init", "-b", &branch]).unwrap();
    git.run(&wt, &["config", "user.email", "t@t"]).unwrap();
    git.run(&wt, &["config", "user.name", "t"]).unwrap();
    git.run(&wt, &["config", "core.hooksPath", "/dev/null"])
        .unwrap();
    std::fs::create_dir_all(wt.join("messages")).unwrap();
    std::fs::write(wt.join("goal.md"), "the goal").unwrap();
    for (name, body) in entries {
        std::fs::write(wt.join("messages").join(name), body).unwrap();
    }
    git.run(&wt, &["add", "-A"]).unwrap();
    git.run(&wt, &["commit", "-m", "fixture"]).unwrap();
    std::fs::create_dir_all(ws.join("steps").join(AGENT).join("001")).unwrap();
    wt
}

/// A parked tail: assistant emitted t1+t2, t1's result committed, t2
/// held — the frontier the resume must respect.
fn parked_tail() -> Vec<(&'static str, String)> {
    let assistant = serde_json::to_string(&[
        Content::ToolUse {
            id: "t1".into(),
            name: "bash".into(),
            input: json!({"command": "true"}),
            signature: None,
        },
        Content::ToolUse {
            id: "t2".into(),
            name: "bash".into(),
            input: json!({"command": "false"}),
            signature: None,
        },
    ])
    .unwrap();
    let t1_result = serde_json::to_string(&[Content::ToolResult {
        tool_use_id: "t1".into(),
        content: vec![Content::Text("ok".into())],
        is_error: false,
    }])
    .unwrap();
    vec![
        ("001-user.md", "hi".to_string()),
        ("002-claude-sonnet-5.json", assistant),
        ("003-tool.json", t1_result),
    ]
}

fn mark_t2(ws: &Path, git: &RealGit) {
    hold::write(
        ws,
        AGENT,
        &hold::Held {
            tool_use_id: "t2".into(),
            tool: "bash".into(),
            reason: "awaiting approval".into(),
        },
        git,
    )
    .unwrap();
}

#[test]
fn a_parked_branch_queues_mail_and_resumes_on_release() {
    // The full cycle, disk-derived at every step: (1) a drive of the
    // parked branch re-adjudicates, holds again, and delivers nothing —
    // the §2.3 pairing survives because mail queues; (2) the control's
    // out-of-band fact flips and the next drive passes: the committed t1
    // is skipped, t2 executes and commits, the mark lifts, and the hop
    // hands off; (3) the successor delivers the queued mail and steps.
    let scripts = TempDir::new().unwrap();
    let control = approval_control(scripts.path());
    let holder = TempDir::new().unwrap();
    let ws = holder.path();
    let wt = parked_workspace(ws, &parked_tail());
    let git = RealGit::new();
    mark_t2(ws, &git);
    let clock = FixedClock::default();
    inbox::deposit(ws, AGENT, "user", "status?", &clock).unwrap();

    let adapter = unreachable_adapter();
    let (sleeper, id) = (StubSleeper::default(), FixedIdGen);
    let tools = StubToolExecutor::ok();
    let rec = RecLauncher::default();
    let mut deps = real_deps(&adapter, &sleeper, &git, &clock, &id, &tools, ws);
    deps.launcher = &rec;
    let mut cfg = || -> Result<WorkerConfig, crate::prompt::Error> {
        Ok(WorkerConfig {
            workflow: Workflow::parse(&gated_workflow(&control), Path::new("workflow.yaml"))
                .unwrap(),
            ..worker_config()
        })
    };

    // (1) Still unapproved: held again, mail untouched, nothing ran.
    let out = run(ws, AGENT, None, &deps, &mut cfg).unwrap();
    assert!(matches!(out, AdvanceOutcome::Held), "got {out:?}");
    assert!(tools.invocations.borrow().is_empty());
    assert!(hold::read(ws, AGENT, &git).is_some());
    assert!(!wt.join("messages/004-user.md").exists(), "mail must queue");
    assert!(rec.invocations.borrow().is_empty());
    assert!(eventually_free(ws, AGENT));

    // (2) Approve out-of-band; the next drive resumes: t1 skipped, t2
    // runs and commits, the mark lifts, the lease rides the handoff.
    std::fs::write(ws.join("approval"), "yes").unwrap();
    let out = run(ws, AGENT, None, &deps, &mut cfg).unwrap();
    let AdvanceOutcome::ToolsPending(lease) = out else {
        panic!("expected ToolsPending, got {out:?}");
    };
    let ran: Vec<String> = tools
        .invocations
        .borrow()
        .iter()
        .map(|c| c.1.clone())
        .collect();
    assert_eq!(ran, vec!["t2"], "only the held frontier executes");
    assert!(wt.join("messages/004-tool.json").exists());
    assert_eq!(hold::read(ws, AGENT, &git), None);
    assert!(
        !wt.join("messages/005-user.md").exists(),
        "mail still queued"
    );
    drop(lease);

    // (3) The successor hop delivers the queued mail and steps to a new
    // final response — the ordinary chain, nothing held anymore.
    let adapter = StubAdapter::scripted([StubAdapter::reply_ok(&happy_response_bytes())]);
    let mut deps = real_deps(&adapter, &sleeper, &git, &clock, &id, &tools, ws);
    deps.launcher = &rec;
    let out = run(ws, AGENT, None, &deps, &mut cfg).unwrap();
    assert!(matches!(out, AdvanceOutcome::Terminal), "got {out:?}");
    assert!(wt.join("messages/005-user.md").exists());
    assert!(wt.join("messages/006-claude-sonnet-5.json").exists());
}

#[test]
fn a_stale_mark_is_cleared_and_the_hop_continues() {
    // The mark names an invocation the tail no longer holds open (a
    // crash between a resumed window's completion and its bookkeeping):
    // swept, then the ordinary no-op hop.
    let holder = TempDir::new().unwrap();
    let ws = holder.path();
    parked_workspace(ws, &super::advance::terminal_tail());
    let git = RealGit::new();
    mark_t2(ws, &git);
    let (clock, id) = (FixedClock::default(), FixedIdGen);
    let (adapter, sleeper) = (unreachable_adapter(), StubSleeper::default());
    let tools = StubToolExecutor::ok();
    let deps = real_deps(&adapter, &sleeper, &git, &clock, &id, &tools, ws);
    let out = run(ws, AGENT, None, &deps, &mut super::advance::no_resolve).unwrap();
    assert!(matches!(out, AdvanceOutcome::NothingToDo), "got {out:?}");
    assert_eq!(hold::read(ws, AGENT, &git), None, "the stale mark is swept");
}

/// Real git with one poisoned subcommand — drives the stale-sweep git
/// failure arm deterministically.
struct FailOn {
    inner: RealGit,
    needle: &'static str,
}
impl GitRunner for FailOn {
    fn run(&self, dest: &Path, args: &[&str]) -> std::io::Result<()> {
        if args.contains(&self.needle) {
            return Err(std::io::Error::other(format!("poisoned {}", self.needle)));
        }
        self.inner.run(dest, args)
    }
    fn run_capture(&self, dest: &Path, args: &[&str]) -> std::io::Result<String> {
        if args.contains(&self.needle) {
            return Err(std::io::Error::other(format!("poisoned {}", self.needle)));
        }
        self.inner.run_capture(dest, args)
    }
}

#[test]
fn a_failed_stale_sweep_surfaces_as_the_git_error_it_is() {
    let holder = TempDir::new().unwrap();
    let ws = holder.path();
    parked_workspace(ws, &super::advance::terminal_tail());
    mark_t2(ws, &RealGit::new());
    let git = FailOn {
        inner: RealGit::new(),
        needle: "-d",
    };
    let (clock, id) = (FixedClock::default(), FixedIdGen);
    let (adapter, sleeper) = (unreachable_adapter(), StubSleeper::default());
    let tools = StubToolExecutor::ok();
    let deps = real_deps(&adapter, &sleeper, &git, &clock, &id, &tools, ws);
    let err = run(ws, AGENT, None, &deps, &mut super::advance::no_resolve).unwrap_err();
    assert!(
        matches!(
            err,
            crate::prompt::Error::Git {
                op: "stale hold mark clear",
                ..
            }
        ),
        "{err:?}"
    );
    // The wrapper's remaining arms, pinned directly: a benign `run`
    // delegates, and a poisoned capture fails like a poisoned run.
    git.run(ws, &["--version"]).unwrap();
    assert!(git.run_capture(ws, &["update-ref", "-d", "x"]).is_err());
}

#[test]
fn a_mark_over_a_missing_worktree_stays_parked() {
    // Kept total: no worktree to resume into — release and exit, the
    // mark (and the park) intact.
    let holder = TempDir::new().unwrap();
    let ws = holder.path();
    RealGit::new()
        .run(ws, &["init", "--bare", "repo.git"])
        .unwrap();
    let git = RealGit::new();
    mark_t2(ws, &git);
    let (clock, id) = (FixedClock::default(), FixedIdGen);
    let (adapter, sleeper) = (unreachable_adapter(), StubSleeper::default());
    let tools = StubToolExecutor::ok();
    let deps = real_deps(&adapter, &sleeper, &git, &clock, &id, &tools, ws);
    let out = run(ws, AGENT, None, &deps, &mut super::advance::no_resolve).unwrap();
    assert!(matches!(out, AdvanceOutcome::NothingToDo), "got {out:?}");
    assert!(hold::read(ws, AGENT, &git).is_some());
}

#[test]
fn a_stop_felling_the_resumes_control_is_the_stopped_terminal() {
    // A stop lands while the resume is mid-consult: the §2.9 cascade
    // fells the control, and the hop concludes the stopped terminal.
    // The mark stays — nothing was adjudicated, so the seam decides
    // nothing — but the stopped exit still settles the window (§2.9
    // step 3), which makes the mark *stale* by the standing definition
    // (its invocation's result is committed): the next drive clears it
    // and the branch resumes as an ordinary stopped one. No new
    // mechanism releases the park; the existing sweep does.
    let scripts = TempDir::new().unwrap();
    let control = scripts.path().join("fixture-control.sh");
    std::fs::write(&control, "#!/bin/sh\nexec sleep 60\n").unwrap();
    std::fs::set_permissions(&control, std::fs::Permissions::from_mode(0o755)).unwrap();
    let holder = TempDir::new().unwrap();
    let ws = holder.path();
    parked_workspace(ws, &parked_tail());
    let git = RealGit::new();
    mark_t2(ws, &git);
    let (clock, id) = (FixedClock::default(), FixedIdGen);
    let (adapter, sleeper) = (unreachable_adapter(), StubSleeper::default());
    let tools = StubToolExecutor::ok();
    let stop = std::sync::atomic::AtomicBool::new(true);
    let rec = RecLauncher::default();
    let mut deps = real_deps(&adapter, &sleeper, &git, &clock, &id, &tools, ws);
    deps.stop = &stop;
    deps.launcher = &rec;
    let mut cfg = || -> Result<WorkerConfig, crate::prompt::Error> {
        Ok(WorkerConfig {
            workflow: Workflow::parse(&gated_workflow(&control), Path::new("workflow.yaml"))
                .unwrap(),
            ..worker_config()
        })
    };
    let out = run(ws, AGENT, None, &deps, &mut cfg).unwrap();
    assert!(matches!(out, AdvanceOutcome::Terminal), "got {out:?}");
    assert!(tools.invocations.borrow().is_empty());
    assert!(hold::read(ws, AGENT, &git).is_some());
    let wt = crate::workspace::agent_worktree(ws, AGENT);
    let settled = std::fs::read_to_string(wt.join("messages/004-tool.json")).expect("settled");
    assert!(settled.contains("t2"), "{settled}");
    assert!(settled.contains("\"is_error\":true"), "{settled}");
}
