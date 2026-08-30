//! End-to-end subprocess tests for `litany bundle` and `litany replay`
//! (ARCH §9.2 *Replay and archival*). These drive the real `git`
//! transport — bundle create, fetch-from-bundle, worktree materialize —
//! against a hand-built workspace, so the argument shapes the unit tests
//! stub out are proven correct on a real repo.

use std::path::{Path, PathBuf};
use std::process::Command;
use tempfile::TempDir;

fn litany_bin() -> std::path::PathBuf {
    crate::test_support::litany_binary()
}

const INHERITED_GIT_ENV: &[&str] = &[
    "GIT_DIR",
    "GIT_WORK_TREE",
    "GIT_INDEX_FILE",
    "GIT_OBJECT_DIRECTORY",
    "GIT_PREFIX",
    "GIT_COMMON_DIR",
    "GIT_ALTERNATE_OBJECT_DIRECTORIES",
];

fn git(dest: &Path, args: &[&str]) {
    let mut cmd = Command::new("git");
    for var in INHERITED_GIT_ENV {
        cmd.env_remove(var);
    }
    let out = cmd.arg("-C").arg(dest).args(args).output().expect("git");
    assert!(
        out.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

const PARENT: &str = "20260101-p1";
const CHILD: &str = "20260101-p1-20260102-c1";
/// A sibling root agent that must NOT be captured by the subtree bundle.
const UNRELATED: &str = "20260101-z9";

/// Build a workspace (§2.2: bare repo.git + config/default) with a
/// parent agent branch carrying a child branch, an unrelated root
/// branch, and diagnostic slices for the subtree.
fn workspace() -> TempDir {
    let ws = TempDir::new().unwrap();
    let repo = ws.path().join("repo.git");
    std::fs::create_dir_all(&repo).unwrap();
    git(&repo, &["init", "-q", "--bare", "-b", "config/default"]);
    git(&repo, &["config", "user.email", "t@test.invalid"]);
    git(&repo, &["config", "user.name", "t"]);
    git(&repo, &["config", "core.hooksPath", "/dev/null"]);
    let author = ws.path().join(".author");
    git(
        &repo,
        &[
            "worktree",
            "add",
            "-q",
            "--orphan",
            "-b",
            "config/default",
            author.to_str().unwrap(),
        ],
    );
    std::fs::write(author.join("version"), "1\n").unwrap();
    git(&author, &["add", "-A"]);
    git(&author, &["commit", "-q", "-m", "config: init"]);
    git(&repo, &["worktree", "remove", author.to_str().unwrap()]);

    // Parent agent branch + its worktree, with a goal work product.
    let parent_wt = ws.path().join("agents").join(PARENT);
    git(
        &repo,
        &[
            "worktree",
            "add",
            "-q",
            parent_wt.to_str().unwrap(),
            "-b",
            &format!("agents/{PARENT}"),
            "config/default",
        ],
    );
    std::fs::write(parent_wt.join("goal.md"), "parent goal\n").unwrap();
    git(&parent_wt, &["add", "-A"]);
    git(&parent_wt, &["commit", "-q", "-m", "parent goal"]);

    // Child branch forked off the parent tip.
    let child_wt = ws.path().join("agents").join(CHILD);
    git(
        &parent_wt,
        &[
            "worktree",
            "add",
            "-q",
            child_wt.to_str().unwrap(),
            "-b",
            &format!("agents/{CHILD}"),
        ],
    );
    std::fs::write(child_wt.join("goal.md"), "child goal\n").unwrap();
    git(&child_wt, &["add", "-A"]);
    git(&child_wt, &["commit", "-q", "-m", "child goal"]);

    git(
        &repo,
        &["branch", &format!("agents/{UNRELATED}"), "config/default"],
    );

    // Diagnostic slices (§2.2): steps for both agents + an inbox message.
    let steps = ws.path().join("steps").join(PARENT).join("001");
    std::fs::create_dir_all(&steps).unwrap();
    std::fs::write(steps.join("meta.json"), "{\"commit\":\"x\"}").unwrap();
    let inbox = ws.path().join("inbox").join(PARENT);
    std::fs::create_dir_all(&inbox).unwrap();
    std::fs::write(inbox.join("user-001.md"), "a message\n").unwrap();
    ws
}

fn run(args: &[&str], home: Option<&Path>) -> std::process::Output {
    let mut cmd = Command::new(litany_bin());
    for var in INHERITED_GIT_ENV {
        cmd.env_remove(var);
    }
    if let Some(h) = home {
        cmd.env("LITANY_HOME", h);
    }
    cmd.args(args).output().expect("litany")
}

#[test]
fn bundle_then_replay_round_trips_the_subtree() {
    let ws = workspace();
    let archive = TempDir::new().unwrap();
    let arch_dir = archive.path().join("arch");

    let out = run(
        &[
            "bundle",
            ws.path().to_str().unwrap(),
            PARENT,
            arch_dir.to_str().unwrap(),
        ],
        None,
    );
    assert!(
        out.status.success(),
        "bundle: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    // One bundle plus two slices (§9.2).
    assert!(arch_dir.join("agents.bundle").exists());
    assert!(
        arch_dir
            .join("steps")
            .join(PARENT)
            .join("001/meta.json")
            .exists()
    );
    assert!(
        arch_dir
            .join("inbox")
            .join(PARENT)
            .join("user-001.md")
            .exists()
    );

    // The bundle carries the subtree, and only the subtree.
    let mut heads_cmd = Command::new("git");
    for var in INHERITED_GIT_ENV {
        heads_cmd.env_remove(var);
    }
    let heads = heads_cmd
        .args([
            "bundle",
            "list-heads",
            arch_dir.join("agents.bundle").to_str().unwrap(),
        ])
        .output()
        .expect("list-heads");
    let heads = String::from_utf8_lossy(&heads.stdout);
    assert!(
        heads.contains(&format!("agents/{PARENT}")),
        "heads: {heads}"
    );
    assert!(heads.contains(&format!("agents/{CHILD}")), "heads: {heads}");
    assert!(
        !heads.contains(UNRELATED),
        "unrelated branch leaked: {heads}"
    );

    // Replay into an isolated scratch home.
    let home = TempDir::new().unwrap();
    let out = run(&["replay", arch_dir.to_str().unwrap()], Some(home.path()));
    assert!(
        out.status.success(),
        "replay: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let scratch = PathBuf::from(String::from_utf8_lossy(&out.stdout).trim().to_string());
    assert_eq!(scratch, home.path().join("replays").join(PARENT));

    // The reconstructed repo has both branches; the primary worktree is
    // materialized under agents/; the slices are restored.
    let scratch_repo = scratch.join("repo.git");
    let mut branch_cmd = Command::new("git");
    for var in INHERITED_GIT_ENV {
        branch_cmd.env_remove(var);
    }
    let branches = branch_cmd
        .arg("-C")
        .arg(&scratch_repo)
        .args(["branch", "--list", "--format=%(refname:short)"])
        .output()
        .expect("branch");
    let branches = String::from_utf8_lossy(&branches.stdout);
    assert!(
        branches.contains(PARENT) && branches.contains(CHILD),
        "branches: {branches}"
    );
    assert_eq!(
        std::fs::read_to_string(scratch.join("agents").join(PARENT).join("goal.md")).unwrap(),
        "parent goal\n"
    );
    assert!(
        scratch
            .join("steps")
            .join(PARENT)
            .join("001/meta.json")
            .exists()
    );
    assert!(
        scratch
            .join("inbox")
            .join(PARENT)
            .join("user-001.md")
            .exists()
    );
}

#[test]
fn bundle_rejects_unknown_agent() {
    let ws = workspace();
    let archive = TempDir::new().unwrap();
    let out = run(
        &[
            "bundle",
            ws.path().to_str().unwrap(),
            "20260101-nope",
            archive.path().join("a").to_str().unwrap(),
        ],
        None,
    );
    assert!(!out.status.success());
    assert!(String::from_utf8_lossy(&out.stderr).contains("no branch matches"));
}

#[test]
fn replay_rejects_missing_bundle() {
    let empty = TempDir::new().unwrap();
    let home = TempDir::new().unwrap();
    let out = run(
        &["replay", empty.path().to_str().unwrap()],
        Some(home.path()),
    );
    assert!(!out.status.success());
    assert!(String::from_utf8_lossy(&out.stderr).contains("not found"));
}

/// In-process coverage of `archive::replay_cli` (the lib wiring the bin
/// delegates to): it resolves the scratch base under `LITANY_HOME`'s data
/// root and lands the scratch workspace there. `LITANY_HOME` is
/// process-global (§2.2); [`crate::test_support::with_litany_home`] is
/// the one lock-guarded mutation every in-process scratch home shares.
#[test]
fn replay_cli_lands_under_litany_home() {
    let ws = workspace();
    let archive = TempDir::new().unwrap();
    let arch_dir = archive.path().join("arch");
    crate::archive::bundle(
        ws.path(),
        PARENT,
        &arch_dir,
        &crate::template::RealGit::new(),
    )
    .expect("bundle");

    let home = TempDir::new().unwrap();
    let scratch = crate::test_support::with_litany_home(home.path(), || {
        crate::archive::replay_cli(&arch_dir).expect("replay_cli")
    });

    assert_eq!(scratch, home.path().join("replays").join(PARENT));
    assert!(scratch.join("repo.git").is_dir());
    assert!(scratch.join("agents").join(PARENT).join("goal.md").exists());
}
