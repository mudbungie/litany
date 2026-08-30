//! The bl-5a36 runaway at the fork, over real git: a parent whose last
//! settled user instruction was itself "spawn a subagent …" dispatches
//! a child — and the child's opening context must carry none of that
//! dialog (ARCH §2.2 branch-scoped, §2.5). Under the defect, the child
//! inherited the instruction as an apparently unanswered user message,
//! obeyed it, and every generation re-dispatched until the operator
//! stopped the tree (yog bl-d023).
//!
//! The compactor half proves the one child exception: its subject *is*
//! the dispatching branch's dialog (§2.7), so its tree keeps it.

use crate::prompt::child_dispatch::{ChildDispatchRequest, run};
use crate::prompt::dispatch::MESSAGES_DIR;
use crate::prompt::dispatch::assembler::assemble;
use crate::prompt::inbox::Launcher;
use crate::template::{GitRunner, RealGit};
use crate::workspace::{self, fixture};
use std::io;
use std::path::{Path, PathBuf};

const PARENT: &str = "20260101-p1";
const USER_TAIL: &str = "spawn a subagent to analyze the files under the XDG location";
const CHILD_GOAL: &str = "Analyze the files under the XDG location and report.\n";

/// A [`Launcher`] that starts nothing: these tests assert on-disk shape,
/// and a real `litany advance` would advance the child underneath them.
struct NoopLauncher;
impl Launcher for NoopLauncher {
    fn launch(&self, _ws: &Path, _agent: &str) -> io::Result<()> {
        Ok(())
    }
}

/// A parent with a settled dialog whose last entry is the runaway
/// instruction, plus a summary and a loaded skill body — the full
/// branch-scoped set a fork drags along (§2.2).
fn parent_with_dialog(ws: &Path) -> PathBuf {
    let g = RealGit::new();
    let parent_wt = fixture::spawn_root(ws, PARENT);
    std::fs::create_dir_all(parent_wt.join(MESSAGES_DIR)).unwrap();
    std::fs::write(parent_wt.join(MESSAGES_DIR).join("001-user.md"), USER_TAIL).unwrap();
    std::fs::create_dir_all(parent_wt.join("summary")).unwrap();
    std::fs::write(parent_wt.join("summary/001.md"), "earlier context\n").unwrap();
    std::fs::create_dir_all(parent_wt.join("skills")).unwrap();
    std::fs::write(parent_wt.join("skills/loaded.md"), "a spent body\n").unwrap();
    g.run(&parent_wt, &["add", "-A"]).unwrap();
    g.run(&parent_wt, &["commit", "-m", "transcript [p1]"])
        .unwrap();
    parent_wt
}

fn dispatch(ws: &Path, parent_wt: &Path, role: &str) -> String {
    run(
        &ChildDispatchRequest {
            repo: ws,
            parent_branch: PARENT,
            parent_worktree: parent_wt,
            role,
            goal: CHILD_GOAL,
            name: None,
            fork_point: None,
            cwd: None,
            pins: crate::prompt::PinnedDocs::none(),
        },
        &RealGit::new(),
        &crate::prompt::SystemClock,
        &crate::prompt::NanoIdGen,
        &NoopLauncher,
        &crate::workspace::agent_name::mint::SplitMix64::from_seed(7),
    )
    .unwrap()
}

fn tree_of(ws: &Path, id: &str) -> String {
    RealGit::new()
        .run_capture(
            &workspace::repo_git(ws),
            &["ls-tree", "-r", "--name-only", &workspace::agent_ref(id)],
        )
        .unwrap()
}

#[test]
fn a_child_never_opens_on_its_dispatchers_dialog() {
    let (_h, ws) = fixture::workspace();
    let parent_wt = parent_with_dialog(&ws);

    let child = dispatch(&ws, &parent_wt, crate::prompt::WORKER_ROLE);

    // The child's dispatch-commit tree carries none of the parent's
    // dialog — no transcript, no summary chain, no skill bodies.
    let listing = tree_of(&ws, &child);
    for gone in ["messages/001-user.md", "summary/001.md", "skills/loaded.md"] {
        assert!(!listing.lines().any(|l| l == gone), "{gone} in {listing}");
    }

    // What the child's first model call opens on is therefore empty of
    // the parent's conversation: the runaway instruction is nowhere,
    // and the only user-side content is the deposited goal, waiting in
    // the child's inbox for its step-1 drain (§2.5).
    let child_wt = workspace::agent_worktree(&ws, &child);
    let history = assemble(&child_wt, None).unwrap();
    assert!(history.is_empty(), "got {history:?}");
    let deposited = std::fs::read_dir(crate::prompt::inbox::inbox_dir(&ws, &child))
        .unwrap()
        .map(|e| std::fs::read_to_string(e.unwrap().path()).unwrap())
        .collect::<String>();
    assert!(deposited.contains(CHILD_GOAL.trim()), "got {deposited:?}");
    assert!(!deposited.contains(USER_TAIL), "got {deposited:?}");

    // The parent's own record is untouched (§2.3 immutability): the
    // dialog stays where it was spoken.
    let parent_listing = tree_of(&ws, PARENT);
    assert!(
        parent_listing.lines().any(|l| l == "messages/001-user.md"),
        "{parent_listing}"
    );
}

#[test]
fn a_compactor_keeps_the_dialog_it_exists_to_compact() {
    let (_h, ws) = fixture::workspace();
    let parent_wt = parent_with_dialog(&ws);

    let child = dispatch(&ws, &parent_wt, crate::prompt::compactor::COMPACTOR_ROLE);

    // §2.7: transcript, summary chain and spent skill bodies are the
    // compactor's input — fork inheritance delivers them, and the prune
    // leaves them.
    let listing = tree_of(&ws, &child);
    for kept in ["messages/001-user.md", "summary/001.md", "skills/loaded.md"] {
        assert!(
            listing.lines().any(|l| l == kept),
            "{kept} not in {listing}"
        );
    }
}
