//! End-to-end subprocess test for `litany dispatch worker`: scaffolds a
//! workspace, fabricates a parent branch + worktree manually, invokes
//! the CLI, and asserts the reshaped dispatch contract (ARCH §2.5 — fork
//! plus front door): a child branch `<parent>-<sub-id>` off the parent's
//! tip carrying the dispatch commit's `goal.md` + `soul.md` (§2.3
//! step 2), plus a clean exit — the deposit + driver launch succeeded.
//!
//! The dispatch now *starts* the child (a detached `litany advance`, §6),
//! which runs asynchronously and may advance the child branch past its
//! dispatch commit. Assertions therefore read only race-free facts: the
//! dispatch commit is an immutable ancestor of the child branch, and
//! `goal.md` / `soul.md` persist across any later delivery commit. The
//! deposit itself is covered deterministically in the `child_dispatch`
//! unit tests; here we prove the CLI wiring end-to-end.

use std::fs;
use std::path::Path;
use std::process::{Command, Stdio};
use tempfile::TempDir;

fn litany_bin() -> &'static str {
    env!("CARGO_BIN_EXE_litany")
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

fn git_command(dest: &Path, args: &[&str]) -> Command {
    let mut cmd = Command::new("git");
    for var in INHERITED_GIT_ENV {
        cmd.env_remove(var);
    }
    cmd.arg("-C").arg(dest).args(args);
    cmd
}

fn git_capture(dest: &Path, args: &[&str]) -> String {
    let out = git_command(dest, args).output().expect("spawn git");
    assert!(
        out.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8(out.stdout).unwrap().trim().to_string()
}

fn git_run(dest: &Path, args: &[&str]) {
    let out = git_command(dest, args).output().expect("spawn git");
    assert!(
        out.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

fn scaffold_repo(dest: &Path, harness: &Path) {
    let out = Command::new(litany_bin())
        .arg("new")
        .arg(dest)
        .env("LITANY_HOME", harness)
        .output()
        .expect("spawn litany new");
    assert!(
        out.status.success(),
        "litany new: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

/// Stand up a parent agent branch + worktree at
/// `<workspace>/agents/<parent>/` so the worker has somewhere to spawn
/// off. Phase 1 doesn't model the parent's prior steps — we just need
/// an `agents/*` ref off `config/default` (§2.3) with a checkout.
fn fabricate_parent(repo: &Path, parent_branch: &str) {
    let bare = repo.join("repo.git");
    let parent_wt = repo.join("agents").join(parent_branch);
    let branch_ref = format!("agents/{parent_branch}");
    git_run(
        &bare,
        &[
            "worktree",
            "add",
            "-b",
            branch_ref.as_str(),
            parent_wt.to_str().unwrap(),
            "config/default",
        ],
    );
}

#[test]
fn dispatch_worker_lands_dispatch_commit_with_goal_and_soul() {
    let holder = TempDir::new().unwrap();
    let harness = holder.path().join("harness");
    fs::create_dir_all(&harness).unwrap();
    let dest = holder.path().join("conv");
    scaffold_repo(&dest, &harness);

    let parent_branch = "parent-conv-id";
    fabricate_parent(&dest, parent_branch);

    let goal_text = "Summarize the parent branch's commits.\n";
    let out = Command::new(litany_bin())
        .args(["dispatch", "worker"])
        .arg(&dest)
        .arg(parent_branch)
        .args(["--goal", goal_text])
        .env("LITANY_HOME", &harness)
        .stderr(Stdio::piped())
        .output()
        .expect("spawn litany dispatch worker");
    assert!(
        out.status.success(),
        "litany dispatch worker: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    // The worker branch is `<parent>-<sub-id>` where `<sub-id>` is
    // `<ts>-<short-id>` (ARCH §2.2). Find it by listing branches that
    // start with the parent prefix and pick the one that isn't the
    // parent itself.
    let bare = dest.join("repo.git");
    let branches = git_capture(
        &bare,
        &[
            "branch",
            "--list",
            "--format=%(refname:short)",
            &format!("agents/{parent_branch}-*"),
        ],
    );
    let sub_branch = branches
        .lines()
        .map(str::trim)
        .filter_map(|b| b.strip_prefix("agents/"))
        .find(|b| !b.is_empty())
        .expect("worker branch listed");
    assert!(
        sub_branch.starts_with(&format!("{parent_branch}-")),
        "got {sub_branch:?}"
    );
    let suffix = &sub_branch[parent_branch.len() + 1..];
    // <ts>-<short-id> = compact-iso (no separators) + 8-char nanoid.
    assert!(suffix.len() >= 9 && suffix.contains('-'), "got {suffix:?}");

    // Child worktree directory present — dispatch materialized it and
    // the launched driver keeps it (§2.3 step 6, disposable but present).
    let sub_wt = dest.join("agents").join(sub_branch);
    assert!(sub_wt.exists(), "child worktree must exist");

    // Goal and soul files committed on the dispatch commit's tree.
    // Read them out of the branch ref so the assertion is robust to
    // worktree mutation in later phases.
    let goal_blob = git_capture(&bare, &["show", &format!("agents/{sub_branch}:goal.md")]);
    assert_eq!(
        goal_blob,
        goal_text.trim_end_matches('\n'),
        "goal.md mismatch"
    );
    let soul_blob = git_capture(&bare, &["show", &format!("agents/{sub_branch}:soul.md")]);
    assert!(
        soul_blob.contains("# Worker"),
        "expected template worker soul, got: {soul_blob}"
    );

    // The dispatch commit is an immutable ancestor of the child branch:
    // find it in history by its subject `dispatch: worker [<sub-branch>]`
    // (race-free — the launched driver only appends past it), and assert
    // it forked off the parent's tip (§2.3 step 2).
    let parent_ref = format!("agents/{parent_branch}");
    let sub_ref = format!("agents/{sub_branch}");
    let parent_tip = git_capture(&bare, &["rev-parse", &parent_ref]);
    let want_subject = format!("dispatch: worker [{sub_branch}]");
    let history = git_capture(&bare, &["log", "--pretty=%H %s", &sub_ref]);
    let dispatch_sha = history
        .lines()
        .find_map(|line| {
            let (sha, subject) = line.split_once(' ')?;
            (subject == want_subject).then(|| sha.to_string())
        })
        .expect("dispatch commit present in child history");
    let dispatch_parents = git_capture(&bare, &["log", "-1", "--pretty=%P", &dispatch_sha]);
    assert_eq!(dispatch_parents.split_whitespace().count(), 1);
    assert_eq!(
        dispatch_parents.split_whitespace().next().unwrap(),
        parent_tip
    );

    // No merge into the parent: the child branch is unmerged against it
    // (children return by message, §2.6 — nothing merges back).
    let unmerged = git_capture(
        &bare,
        &[
            "branch",
            "--list",
            &format!("agents/{parent_branch}-*"),
            "--no-merged",
            &parent_ref,
        ],
    );
    assert!(unmerged.contains(sub_branch), "got {unmerged:?}");
}

#[test]
fn dispatch_worker_rejects_missing_goal() {
    let holder = TempDir::new().unwrap();
    let harness = holder.path().join("harness");
    fs::create_dir_all(&harness).unwrap();
    let dest = holder.path().join("conv");
    scaffold_repo(&dest, &harness);
    fabricate_parent(&dest, "p1");

    let out = Command::new(litany_bin())
        .args(["dispatch", "worker"])
        .arg(&dest)
        .arg("p1")
        .env("LITANY_HOME", &harness)
        .stderr(Stdio::piped())
        .output()
        .expect("spawn litany dispatch worker");
    assert!(!out.status.success(), "expected failure on missing --goal");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("--goal is required"),
        "got stderr: {stderr}"
    );
}

#[test]
fn dispatch_compactor_rejects_unexpected_goal() {
    let holder = TempDir::new().unwrap();
    let harness = holder.path().join("harness");
    fs::create_dir_all(&harness).unwrap();
    let dest = holder.path().join("conv");
    scaffold_repo(&dest, &harness);
    fabricate_parent(&dest, "p1");

    let out = Command::new(litany_bin())
        .args(["dispatch", "compactor"])
        .arg(&dest)
        .arg("p1")
        .args(["--goal", "no goal allowed for compactor"])
        .env("LITANY_HOME", &harness)
        .stderr(Stdio::piped())
        .output()
        .expect("spawn litany dispatch compactor");
    assert!(!out.status.success(), "expected failure on stray --goal");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("--goal is not accepted"),
        "got stderr: {stderr}"
    );
}

#[test]
fn dispatch_role_absent_from_config_is_refused() {
    // The role set is open (§4.3): a role the governing config commit does
    // not define — here `verifier`, absent from the default template — is
    // refused by the config-validity check, which names the config commit
    // consulted. Not a hard-coded name list; the same check that *accepts*
    // a verifier the config adds (zero code).
    let holder = TempDir::new().unwrap();
    let harness = holder.path().join("harness");
    fs::create_dir_all(&harness).unwrap();
    let dest = holder.path().join("conv");
    scaffold_repo(&dest, &harness);
    fabricate_parent(&dest, "p1");

    let out = Command::new(litany_bin())
        .args(["dispatch", "verifier"])
        .arg(&dest)
        .arg("p1")
        .args(["--goal", "judge it"])
        .env("LITANY_HOME", &harness)
        .stderr(Stdio::piped())
        .output()
        .expect("spawn litany dispatch verifier");
    assert!(!out.status.success(), "expected failure on undefined role");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("role \"verifier\" is not defined in"),
        "got stderr: {stderr}"
    );
}
