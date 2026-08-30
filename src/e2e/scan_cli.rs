//! End-to-end subprocess tests for the operator verb `litany scan`
//! (ARCH §2.11 *Crashes are a failure class*, §8) and for its removal
//! from the driver hot paths: `litany prompt` and `litany dispatch`
//! never run the workspace-wide sweep.

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

/// A root-shaped agent id and one of its children (§2.3 token shape).
const PARENT: &str = "20260101-p1";
const CHILD: &str = "20260101-p1-20260102-c1";

/// A workspace whose git state shows a hard-crashed child: a bare
/// repo.git with a config/default root (§2.2), a parent agent branch,
/// and a child agent branch that never deposited a result — the §8
/// sweep's candidate.
fn workspace_with_crashed_child() -> TempDir {
    let ws = TempDir::new().unwrap();
    let repo = ws.path().join("repo.git");
    std::fs::create_dir_all(&repo).unwrap();
    git(&repo, &["init", "--bare", "-b", "config/default"]);
    let author = ws.path().join(".author");
    let author_str = author.to_string_lossy().to_string();
    git(
        &repo,
        &[
            "worktree",
            "add",
            "--orphan",
            "-b",
            "config/default",
            author_str.as_str(),
        ],
    );
    std::fs::write(author.join("version"), "1\n").unwrap();
    git(&author, &["add", "-A"]);
    git(&author, &["config", "user.email", "t@test.invalid"]);
    git(&author, &["config", "user.name", "t"]);
    git(&author, &["config", "core.hooksPath", "/dev/null"]);
    git(&author, &["commit", "-m", "config: init"]);
    git(&repo, &["worktree", "remove", author_str.as_str()]);
    git(
        &repo,
        &["branch", &format!("agents/{PARENT}"), "config/default"],
    );
    git(
        &repo,
        &["branch", &format!("agents/{CHILD}"), "config/default"],
    );
    ws
}

fn died_deposit(ws: &Path) -> PathBuf {
    ws.join("inbox")
        .join(PARENT)
        .join(format!("{CHILD}-001.md"))
}

#[test]
fn scan_verb_heals_a_crash_stranded_child() {
    let ws = workspace_with_crashed_child();
    // Hold the parent's executor lock across the scan: the sweep's
    // deposit is lock-free and still lands, while the flush observes a
    // driven branch and leaves it alone (§2.11) — so the deposit is
    // still in the inbox for this test to read. The flush's real
    // driver launch is exercised end-to-end in `advance_cli.rs`.
    let parent_inbox = ws.path().join("inbox").join(PARENT);
    let _held = crate::prompt::inbox::try_acquire(&parent_inbox)
        .unwrap()
        .expect("free");
    let out = Command::new(litany_bin())
        .arg("scan")
        .arg(ws.path())
        .output()
        .expect("spawn litany scan");
    assert!(
        out.status.success(),
        "litany scan: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    // The operator summary on stdout.
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("silent deaths: 1"), "got {stdout:?}");
    // The healing: a died-epitaph result message deposited on the
    // crashed child's behalf, into its parent's inbox.
    let body = std::fs::read_to_string(died_deposit(ws.path())).unwrap();
    assert!(body.contains("epitaph: died"), "got {body:?}");
    assert!(body.contains(&format!("from: {CHILD}")), "got {body:?}");
}

#[test]
fn scan_verb_surfaces_a_broken_workspace_loudly() {
    // Not a workspace (no repo.git) → the §2.2 layout guard refuses →
    // non-zero exit (an operator verb is loud, §2.11).
    let ws = TempDir::new().unwrap();
    let out = Command::new(litany_bin())
        .arg("scan")
        .arg(ws.path())
        .output()
        .expect("spawn litany scan");
    assert!(!out.status.success());
    assert!(String::from_utf8_lossy(&out.stderr).contains("litany scan"));
}

#[test]
fn scan_names_a_root_whose_model_call_failed() {
    // bl-ee80 live-wire shape: a root's model call errors non-retryably
    // — the segment closes cleanly (`error` then `end`), so a
    // no-terminal-`end` test reads the branch as idle while it can never
    // advance (§2.10). The sweep classifies it dead from the same
    // framing tail, and — a root having no parent inbox for a deposit —
    // surfaces it *by name* in the operator summary.
    let ws = workspace_with_crashed_child();
    // Hold the child's lock so only the root's own death is in view (a
    // driven branch is never swept) and no deposit fills the parent's
    // inbox — the flush then launches nothing from this pass.
    let child_inbox = ws.path().join("inbox").join(CHILD);
    let _held = crate::prompt::inbox::try_acquire(&child_inbox)
        .unwrap()
        .expect("free");
    let step = ws.path().join("steps").join(PARENT).join("001");
    std::fs::create_dir_all(&step).unwrap();
    std::fs::write(
        step.join("response.json"),
        "{\"type\":\"error\",\"kind\":\"parse_input\",\"message\":\"user accepts only text content\"}\n{\"type\":\"end\"}\n",
    )
    .unwrap();
    let out = Command::new(litany_bin())
        .arg("scan")
        .arg(ws.path())
        .output()
        .expect("spawn litany scan");
    assert!(
        out.status.success(),
        "litany scan: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains(&format!("silent deaths: 1 ({PARENT})")),
        "got {stdout:?}"
    );
    assert!(
        !died_deposit(ws.path()).exists(),
        "a root gets no deposit — the name is the surfacing"
    );
}

#[test]
fn prompt_hot_path_runs_no_workspace_scan() {
    // The same crashed-child workspace, touched by a driver instead of
    // the operator verb. `litany prompt` fails fast here (LITANY_HOME has
    // no models.yaml), but the point is what it must NOT have done first:
    // before bl-5846 the startup scan ran ahead of config load and would
    // have deposited the died epitaph; now no deposit may appear.
    let ws = workspace_with_crashed_child();
    let harness = TempDir::new().unwrap();
    let out = Command::new(litany_bin())
        .arg("prompt")
        .arg(ws.path())
        .arg("hi")
        .env("LITANY_HOME", harness.path())
        .output()
        .expect("spawn litany prompt");
    assert!(!out.status.success(), "prompt fails on the empty harness");
    assert!(
        !died_deposit(ws.path()).exists(),
        "litany prompt must not sweep the workspace (§2.11)"
    );
}

#[test]
fn dispatch_hot_path_runs_no_workspace_scan() {
    let ws = workspace_with_crashed_child();
    let out = Command::new(litany_bin())
        .args(["dispatch", "no-such-role"])
        .arg(ws.path())
        .arg(PARENT)
        .output()
        .expect("spawn litany dispatch");
    assert!(!out.status.success(), "a config-undefined role is refused");
    assert!(
        !died_deposit(ws.path()).exists(),
        "litany dispatch must not sweep the workspace (§2.11)"
    );
}
