//! Integration tests: `litany stop` idempotence + error paths
//! (companion to `tests/stop_cli.rs`'s cascade test).

use super::stop_common::{git_run, litany_bin, repo_git, scaffold_repo, write_global_models};
use std::fs;
use std::process::{Command, Stdio};
use tempfile::TempDir;

/// Fork a stale agent branch `agents/<id>` off `config/default` with a
/// dispatch-shaped commit, then tear its worktree down — the state a
/// crashed or finished agent leaves behind (§2.3 step 6).
fn stale_agent(dest: &std::path::Path, id: &str) {
    let repo = repo_git(dest);
    let wt = dest.join("agents").join(id);
    let wt_str = wt.to_string_lossy().to_string();
    let branch = format!("agents/{id}");
    git_run(
        &repo,
        &[
            "worktree",
            "add",
            "-b",
            branch.as_str(),
            wt_str.as_str(),
            "config/default",
        ],
    );
    fs::write(wt.join("goal.md"), "g").unwrap();
    git_run(&wt, &["add", "goal.md"]);
    git_run(
        &wt,
        &[
            "-c",
            "user.email=t@e",
            "-c",
            "user.name=T",
            "-c",
            "core.hooksPath=/dev/null",
            "commit",
            "-m",
            "dispatch",
        ],
    );
    git_run(&repo, &["worktree", "remove", "--force", wt_str.as_str()]);
}

#[test]
fn stop_on_branch_with_no_live_writer_is_idempotent_success() {
    let holder = TempDir::new().unwrap();
    let harness = holder.path().join("harness");
    fs::create_dir_all(&harness).unwrap();
    write_global_models(&harness);
    let dest = holder.path().join("conv");
    scaffold_repo(&dest, &harness);
    stale_agent(&dest, "20260101-st22");

    let stop_out = Command::new(litany_bin())
        .arg("stop")
        .arg(&dest)
        .arg("20260101-st22")
        .stderr(Stdio::piped())
        .output()
        .expect("spawn litany stop");
    assert!(
        stop_out.status.success(),
        "litany stop must succeed idempotently: {}",
        String::from_utf8_lossy(&stop_out.stderr)
    );
}

#[test]
fn stop_stop_children_flag_parses_and_is_idempotent() {
    // The `--stop-children` id-namespace walk (§2.9) reaches the same
    // no-holder short-circuit as a bare stop when nothing is driving —
    // proving the flag parses and plumbs through `cli_run` end-to-end.
    let holder = TempDir::new().unwrap();
    let harness = holder.path().join("harness");
    fs::create_dir_all(&harness).unwrap();
    write_global_models(&harness);
    let dest = holder.path().join("conv");
    scaffold_repo(&dest, &harness);
    stale_agent(&dest, "20260101-st77");

    let stop_out = Command::new(litany_bin())
        .arg("stop")
        .arg(&dest)
        .arg("20260101-st77")
        .arg("--stop-children")
        .stderr(Stdio::piped())
        .output()
        .expect("spawn litany stop --stop-children");
    assert!(
        stop_out.status.success(),
        "litany stop --stop-children must succeed idempotently: {}",
        String::from_utf8_lossy(&stop_out.stderr)
    );
}

#[test]
fn stop_on_missing_branch_errors() {
    let holder = TempDir::new().unwrap();
    let harness = holder.path().join("harness");
    fs::create_dir_all(&harness).unwrap();
    write_global_models(&harness);
    let dest = holder.path().join("conv");
    scaffold_repo(&dest, &harness);

    let out = Command::new(litany_bin())
        .arg("stop")
        .arg(&dest)
        .arg("does-not-exist")
        .stderr(Stdio::piped())
        .output()
        .expect("spawn litany stop");
    assert!(!out.status.success(), "expected nonzero exit");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("does-not-exist") && stderr.contains("does not exist"),
        "got: {stderr}"
    );
}

#[test]
fn stop_refuses_the_retired_per_conversation_layout() {
    // Pre-v1 clean break (§2.2, §10): the old layout is refused with an
    // actionable error naming both the found and the current shape.
    let holder = TempDir::new().unwrap();
    let dest = holder.path().join("old-conv");
    fs::create_dir_all(dest.join("root/.git")).unwrap();
    fs::write(dest.join("providers.yaml"), "roles: {}\n").unwrap();

    let out = Command::new(litany_bin())
        .arg("stop")
        .arg(&dest)
        .arg("20260101-x1")
        .stderr(Stdio::piped())
        .output()
        .expect("spawn litany stop");
    assert!(!out.status.success(), "expected nonzero exit");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("retired per-conversation layout") && stderr.contains("litany new"),
        "got: {stderr}"
    );
}
