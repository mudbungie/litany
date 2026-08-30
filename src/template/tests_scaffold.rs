//! Integration test for the workspace template, exercised end-to-end
//! through the `litany new` subcommand.
//!
//! Validates the resulting workspace against ARCH §2.2: one bare
//! repository at `repo.git`, exactly one ref — `config/default`, no
//! `main` — whose head commit carries the control files (`manifest.yaml`,
//! `workflow.yaml`, `providers.yaml`, `version`, `souls/`) and the
//! `descriptions/**` snapshot.

use crate::config::manifest::{Manifest, OverflowPolicy};
use crate::config::per_repo_providers::PerRepoProviders;
use crate::config::version::{self, Version};
use crate::config::workflow::Workflow;
use std::path::{Path, PathBuf};
use std::process::Command;
use tempfile::TempDir;

/// Git env vars that a hook-invoked test may inherit. They would cause
/// subcommands to operate on the outer repo instead of the scaffolded
/// tempdir, so scrub them from every `git` we spawn here.
const INHERITED_GIT_ENV: &[&str] = &[
    "GIT_DIR",
    "GIT_WORK_TREE",
    "GIT_INDEX_FILE",
    "GIT_OBJECT_DIRECTORY",
    "GIT_PREFIX",
    "GIT_COMMON_DIR",
    "GIT_ALTERNATE_OBJECT_DIRECTORIES",
];

fn scrub_git_env(cmd: &mut Command) {
    for var in INHERITED_GIT_ENV {
        cmd.env_remove(var);
    }
}

fn litany_bin() -> PathBuf {
    crate::test_support::litany_binary()
}

fn scaffold(dest: &Path) -> String {
    // Point the harness root at a throwaway dir: `new` founds the root
    // it resolves (ARCH §2.2), so the descriptions-always producer
    // (§3.3) sees the shipped pools rather than the dev host's, and the
    // dev host's own install is left alone.
    let home = TempDir::new().unwrap();
    let mut cmd = Command::new(litany_bin());
    scrub_git_env(&mut cmd);
    let out = cmd
        .arg("new")
        .arg(dest)
        .env("LITANY_HOME", home.path())
        .env("GIT_AUTHOR_NAME", "litany-test")
        .env("GIT_AUTHOR_EMAIL", "test@example.invalid")
        .env("GIT_COMMITTER_NAME", "litany-test")
        .env("GIT_COMMITTER_EMAIL", "test@example.invalid")
        // A fixture identity is not this machine's, and a global
        // `core.hooksPath` hook that enforces one would refuse every
        // commit the spawned binary makes. Override the setting for
        // this child only; a nonexistent path means no hooks.
        .env("GIT_CONFIG_COUNT", "1")
        .env("GIT_CONFIG_KEY_0", "core.hooksPath")
        .env("GIT_CONFIG_VALUE_0", "/dev/null")
        .output()
        .expect("invoke litany binary");
    assert!(
        out.status.success(),
        "litany new failed: {}\nstderr: {}",
        out.status,
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8(out.stdout).unwrap().trim().to_string()
}

fn scaffolded() -> (TempDir, PathBuf) {
    let holder = TempDir::new().unwrap();
    let dest = holder.path().join("ws");
    let stdout = scaffold(&dest);
    assert_eq!(stdout, dest.display().to_string(), "stdout must echo path");
    (holder, dest)
}

/// Read `<path>` out of the config commit's tree — the §2.2 control
/// home (`git show config/default:<path>` against the bare repo.git).
fn show_control(ws: &Path, path: &str) -> String {
    let mut cmd = Command::new("git");
    scrub_git_env(&mut cmd);
    let out = cmd
        .arg("-C")
        .arg(ws.join("repo.git"))
        .args(["show", &format!("config/default:{path}")])
        .output()
        .expect("spawn git show");
    assert!(
        out.status.success(),
        "git show {path}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8(out.stdout).unwrap()
}

/// Parse a control file through its loader, from the config commit's
/// tree (the loaders' `parse` seams exist for exactly this, §2.2).
fn parse_origin(path: &str) -> PathBuf {
    PathBuf::from(format!("config/default:{path}"))
}

#[test]
fn version_file_is_one() {
    let (_holder, ws) = scaffolded();
    let raw = show_control(&ws, "version");
    assert_eq!(raw.trim(), "1");
    // The parser agrees when handed the same content the runtime reads
    // out of the config commit — and accepts it, so the scaffolded
    // template is a version this harness supports (ARCH §10).
    assert_eq!(
        Version::parse(&raw, &parse_origin("version")).unwrap(),
        Version(version::SUPPORTED)
    );
}

#[test]
fn providers_yaml_is_roles_only_and_validates() {
    let (_holder, ws) = scaffolded();
    let raw = show_control(&ws, "providers.yaml");
    let per_repo = PerRepoProviders::parse(&raw, &parse_origin("providers.yaml")).unwrap();
    assert!(per_repo.roles.contains_key("worker"));
    assert!(per_repo.roles.contains_key("compactor"));
    assert_eq!(per_repo.roles["worker"].provider, "anthropic");
    assert_eq!(per_repo.roles["worker"].model, "claude-sonnet-5");
}

#[test]
fn manifest_yaml_is_role_keyed_per_arch_5_2() {
    let (_holder, ws) = scaffolded();
    let raw = show_control(&ws, "manifest.yaml");
    let manifest = Manifest::parse(&raw, &parse_origin("manifest.yaml")).unwrap();
    assert!(manifest.roles.contains_key("worker"));
    assert!(manifest.roles.contains_key("compactor"));
    let worker = &manifest.roles["worker"];
    assert!(worker.budget_tokens > 0);
    assert!(worker.pinned.iter().any(|p| p == "goal.md"));
    assert!(worker.pinned.iter().any(|p| p == "soul.md"));
    // ARCH §5.2 amended (v0.3.1): step records are not context, so
    // `worker.order` carries no `steps/**` entries.
    assert!(
        !worker.order.iter().any(|p| p.starts_with("steps/")),
        "worker.order must not reference steps/** (§5.2 amended)"
    );
    assert_eq!(worker.overflow, OverflowPolicy::DropOldestSummaries);
    assert_eq!(
        manifest.roles["compactor"].overflow,
        OverflowPolicy::Truncate
    );
}

#[test]
fn workflow_yaml_validates() {
    let (_holder, ws) = scaffolded();
    let raw = show_control(&ws, "workflow.yaml");
    Workflow::parse(&raw, &parse_origin("workflow.yaml")).unwrap();
}

#[test]
fn souls_live_in_the_config_commit() {
    let (_holder, ws) = scaffolded();
    assert!(show_control(&ws, "souls/worker.md").contains("# Worker"));
    assert!(!show_control(&ws, "souls/compactor.md").is_empty());
}

#[test]
fn workspace_has_exactly_one_ref_and_no_main() {
    // ARCH §2.2–§2.3: no `main`, no trunk — the only ref in a fresh
    // workspace is the config branch, and the repository is bare.
    let (_holder, ws) = scaffolded();
    let repo = ws.join("repo.git");
    assert!(repo.is_dir());
    let mut cmd = Command::new("git");
    scrub_git_env(&mut cmd);
    let out = cmd
        .arg("-C")
        .arg(&repo)
        .args(["for-each-ref", "--format=%(refname)"])
        .output()
        .unwrap();
    assert!(out.status.success());
    assert_eq!(
        String::from_utf8(out.stdout).unwrap().trim(),
        "refs/heads/config/default"
    );
    let mut bare = Command::new("git");
    scrub_git_env(&mut bare);
    let out = bare
        .arg("-C")
        .arg(&repo)
        .args(["rev-parse", "--is-bare-repository"])
        .output()
        .unwrap();
    assert_eq!(String::from_utf8(out.stdout).unwrap().trim(), "true");
}

#[test]
fn config_commit_is_an_orphan_root_with_one_commit() {
    let (_holder, ws) = scaffolded();
    let repo = ws.join("repo.git");
    let mut log = Command::new("git");
    scrub_git_env(&mut log);
    let log_out = log
        .arg("-C")
        .arg(&repo)
        .args(["log", "--oneline", "config/default"])
        .output()
        .unwrap();
    assert!(log_out.status.success(), "git log failed");
    let text = String::from_utf8(log_out.stdout).unwrap();
    assert_eq!(text.lines().count(), 1, "expected one commit, got:\n{text}");
    assert!(text.contains("config: init [config/default]"));
}

#[test]
fn no_stray_files_or_checkouts_remain() {
    // The authoring checkout is torn down (§2.2): the workspace holds
    // only repo.git after creation.
    let (_holder, ws) = scaffolded();
    let entries: Vec<String> = std::fs::read_dir(&ws)
        .unwrap()
        .flatten()
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .collect();
    assert_eq!(entries, vec!["repo.git".to_string()], "got {entries:?}");
}

#[test]
fn no_args_uses_harness_root_with_auto_id() {
    // `litany new` with no path argument resolves
    // <LITANY_HOME>/workspaces/<auto-id>/ and prints that path.
    let home = TempDir::new().unwrap();
    let mut cmd = Command::new(litany_bin());
    scrub_git_env(&mut cmd);
    let out = cmd
        .arg("new")
        .env("LITANY_HOME", home.path())
        .env("GIT_AUTHOR_NAME", "litany-test")
        .env("GIT_AUTHOR_EMAIL", "test@example.invalid")
        .env("GIT_COMMITTER_NAME", "litany-test")
        .env("GIT_COMMITTER_EMAIL", "test@example.invalid")
        // A fixture identity is not this machine's, and a global
        // `core.hooksPath` hook that enforces one would refuse every
        // commit the spawned binary makes. Override the setting for
        // this child only; a nonexistent path means no hooks.
        .env("GIT_CONFIG_COUNT", "1")
        .env("GIT_CONFIG_KEY_0", "core.hooksPath")
        .env("GIT_CONFIG_VALUE_0", "/dev/null")
        .output()
        .expect("invoke litany binary");
    assert!(
        out.status.success(),
        "litany new (no args) failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let printed = String::from_utf8(out.stdout).unwrap().trim().to_string();
    let printed_path = PathBuf::from(&printed);
    assert!(printed_path.starts_with(home.path().join("workspaces")));
    assert!(printed_path.join("repo.git").is_dir());
}

#[test]
fn binary_refuses_non_empty_destination() {
    let holder = TempDir::new().unwrap();
    let dest = holder.path().join("occupied");
    std::fs::create_dir(&dest).unwrap();
    std::fs::write(dest.join("preexisting"), b"x").unwrap();
    let home = TempDir::new().unwrap();
    let mut cmd = Command::new(litany_bin());
    scrub_git_env(&mut cmd);
    let out = cmd
        .arg("new")
        .arg(&dest)
        .env("LITANY_HOME", home.path())
        .output()
        .unwrap();
    assert!(
        !out.status.success(),
        "binary should refuse non-empty destination"
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("not empty"), "unexpected stderr: {stderr}");
}

#[test]
fn binary_accepts_existing_empty_destination() {
    let holder = TempDir::new().unwrap();
    let dest = holder.path().join("preexisting-empty");
    std::fs::create_dir(&dest).unwrap();
    let stdout = scaffold(&dest);
    assert_eq!(stdout, dest.display().to_string());
    assert!(dest.join("repo.git").is_dir());
}
