//! Tests for the reshaped child dispatch (fork + front door, §2.5).

use super::*;
use crate::prompt::inbox;
use crate::workspace::fixture;
use std::cell::RefCell;
use std::io;
use std::path::PathBuf;

/// A [`Launcher`] that records its launches instead of spawning a real
/// `litany advance`. `fail` makes `launch` return an error so the
/// post-deposit error arm is exercised deterministically.
pub(super) struct RecordingLauncher {
    pub(super) invocations: RefCell<Vec<(PathBuf, String)>>,
    fail: bool,
}

impl RecordingLauncher {
    pub(super) fn ok() -> Self {
        Self {
            invocations: RefCell::new(Vec::new()),
            fail: false,
        }
    }
    pub(super) fn failing() -> Self {
        Self {
            invocations: RefCell::new(Vec::new()),
            fail: true,
        }
    }
}

impl Launcher for RecordingLauncher {
    fn launch(&self, workspace: &Path, agent_id: &str) -> io::Result<()> {
        self.invocations
            .borrow_mut()
            .push((workspace.to_path_buf(), agent_id.to_string()));
        if self.fail {
            Err(io::Error::other("stub launch refused"))
        } else {
            Ok(())
        }
    }
}

/// A seeded mint RNG (§2.3): every dispatch settles a name, so the
/// omission path mints deterministically under test.
pub(super) fn test_rng() -> crate::workspace::agent_name::mint::SplitMix64 {
    crate::workspace::agent_name::mint::SplitMix64::from_seed(7)
}

pub(super) fn req<'a>(
    repo: &'a Path,
    parent: &'a str,
    wt: &'a Path,
    goal: &'a str,
) -> ChildDispatchRequest<'a> {
    ChildDispatchRequest {
        repo,
        parent_branch: parent,
        parent_worktree: wt,
        role: crate::prompt::WORKER_ROLE,
        goal,
        name: None,
        fork_point: None,
        cwd: None,
        pins: crate::prompt::PinnedDocs::none(),
    }
}

#[test]
fn forks_the_child_pins_the_goal_and_deposits_through_the_front_door() {
    // Real git end to end: the child's soul is read from the parent's
    // governing config (§2.2), the branch is `agents/<parent>-<sub-id>`,
    // the dispatch commit removed the control files, `goal.md` is pinned,
    // the dispatch message is deposited from the parent, and the driver
    // was launched at the child's id (§2.5, §2.11).
    let (_h, ws) = fixture::workspace();
    fixture::amend_config(&ws, &[("souls/worker.md", "worker soul body\n")]);
    let parent_wt = fixture::spawn_root(&ws, "20260101-p1");
    let g = crate::template::RealGit::new();
    let launcher = RecordingLauncher::ok();
    let child = run(
        &req(&ws, "20260101-p1", &parent_wt, "do the thing\n"),
        &g,
        &crate::prompt::SystemClock,
        &crate::prompt::NanoIdGen,
        &launcher,
        &test_rng(),
    )
    .unwrap();
    assert!(child.starts_with("20260101-p1-"), "{child}");

    let child_wt = workspace::agent_worktree(&ws, &child);
    assert_eq!(
        std::fs::read_to_string(child_wt.join("goal.md")).unwrap(),
        "do the thing\n"
    );
    // `show_control` trims surrounding whitespace, so the soul lands
    // trimmed — the dispatch commit still pinned a `soul.md` (§2.3 step 2).
    assert_eq!(
        std::fs::read_to_string(child_wt.join("soul.md")).unwrap(),
        "worker soul body"
    );
    // The ref namespace holds the child, and its tree carries no control
    // files (§2.2 — the dispatch removed them).
    let ids = workspace::agent_ids(&ws, &g).unwrap();
    assert!(ids.contains(&child), "{ids:?}");
    assert!(!child_wt.join("providers.yaml").exists());
    assert!(!child_wt.join("souls").exists());

    // The dispatch message was deposited from the parent into the child's
    // inbox — the sender token is the dispatcher's id (§2.11 provenance).
    let inbox_dir = inbox::inbox_dir(&ws, &child);
    let deposited: Vec<_> = std::fs::read_dir(&inbox_dir)
        .unwrap()
        .flatten()
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| n.starts_with("20260101-p1-"))
        .collect();
    assert_eq!(deposited.len(), 1, "{deposited:?}");
    let body = std::fs::read_to_string(inbox_dir.join(&deposited[0])).unwrap();
    assert!(body.contains("do the thing"), "{body}");
    assert!(body.contains("from: 20260101-p1"), "{body}");

    // The front door launched the child's driver exactly once, at its id.
    let invocations = launcher.invocations.borrow();
    assert_eq!(invocations.len(), 1);
    assert_eq!(invocations[0], (ws.clone(), child.clone()));
}

#[test]
fn a_failing_launch_surfaces_as_executor_lock() {
    // The deposit succeeded; the front-door launch failed. The error is
    // surfaced against the child's inbox rather than swallowed.
    let (_h, ws) = fixture::workspace();
    let parent_wt = fixture::spawn_root(&ws, "20260101-p1");
    let g = crate::template::RealGit::new();
    let launcher = RecordingLauncher::failing();
    let err = run(
        &req(&ws, "20260101-p1", &parent_wt, "g"),
        &g,
        &crate::prompt::SystemClock,
        &crate::prompt::NanoIdGen,
        &launcher,
        &test_rng(),
    )
    .unwrap_err();
    assert!(matches!(err, Error::ExecutorLock { .. }), "got {err:?}");
}

#[test]
fn a_broken_child_inbox_surfaces_as_deposit() {
    // With `<ws>/inbox` occupied by a file, the child inbox directory
    // cannot be created and the deposit fails after the fork succeeded.
    let (_h, ws) = fixture::workspace();
    let parent_wt = fixture::spawn_root(&ws, "20260101-p1");
    std::fs::write(ws.join(inbox::INBOX_DIR), b"not a dir").unwrap();
    let g = crate::template::RealGit::new();
    let launcher = RecordingLauncher::ok();
    let err = run(
        &req(&ws, "20260101-p1", &parent_wt, "g"),
        &g,
        &crate::prompt::SystemClock,
        &crate::prompt::NanoIdGen,
        &launcher,
        &test_rng(),
    )
    .unwrap_err();
    assert!(matches!(err, Error::Deposit(_)), "got {err:?}");
    // The fork still happened — the failure is post-fork (§2.5 order).
    assert!(launcher.invocations.borrow().is_empty());
}

#[test]
fn missing_soul_is_surfaced_as_control_read_before_any_spawn() {
    // A parent forked off a config that never carried `souls/worker.md`
    // fails the soul read loudly, before any fork side effect.
    let (_h, ws) = fixture::workspace();
    let g = crate::template::RealGit::new();
    let author = ws.join(".strip");
    let author_str = author.to_string_lossy().to_string();
    g.run(
        &workspace::repo_git(&ws),
        &["worktree", "add", author_str.as_str(), "config/default"],
    )
    .unwrap();
    g.run(&author, &["rm", "-r", "-q", "souls"]).unwrap();
    g.run(&author, &["commit", "-m", "config: no souls"])
        .unwrap();
    g.run(
        &workspace::repo_git(&ws),
        &["worktree", "remove", "--force", author_str.as_str()],
    )
    .unwrap();
    let parent_wt = fixture::spawn_root(&ws, "20260101-p2");
    let launcher = RecordingLauncher::ok();
    let err = run(
        &req(&ws, "20260101-p2", &parent_wt, "g"),
        &g,
        &crate::prompt::SystemClock,
        &crate::prompt::NanoIdGen,
        &launcher,
        &test_rng(),
    )
    .unwrap_err();
    assert!(matches!(err, Error::ControlRead { .. }), "got {err:?}");
    // Only the parent exists — no child branch was spawned.
    assert_eq!(workspace::agent_ids(&ws, &g).unwrap().len(), 1);
    assert!(launcher.invocations.borrow().is_empty());
}

#[test]
fn a_parent_with_no_config_ancestor_fails_as_git() {
    // An orphan parent branch has no governing config; the followed-config derivation
    // fails loudly (§2.2) before any fork.
    let (_h, ws) = fixture::workspace();
    let g = crate::template::RealGit::new();
    let wt = workspace::agent_worktree(&ws, "20260101-x1");
    let wt_str = wt.to_string_lossy().to_string();
    g.run(
        &workspace::repo_git(&ws),
        &[
            "worktree",
            "add",
            "--orphan",
            "-b",
            "agents/20260101-x1",
            wt_str.as_str(),
        ],
    )
    .unwrap();
    std::fs::write(wt.join("goal.md"), "g").unwrap();
    g.run(&wt, &["add", "goal.md"]).unwrap();
    g.run(&wt, &["commit", "-m", "orphan"]).unwrap();
    let launcher = RecordingLauncher::ok();
    let err = run(
        &req(&ws, "20260101-x1", &wt, "g"),
        &g,
        &crate::prompt::SystemClock,
        &crate::prompt::NanoIdGen,
        &launcher,
        &test_rng(),
    )
    .unwrap_err();
    assert!(
        matches!(
            err,
            Error::Git {
                op: "followed config",
                ..
            }
        ),
        "got {err:?}"
    );
}

mod budget;
mod cwd;
mod edges;
mod naming;
