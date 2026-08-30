//! End-to-end `litany config` (ARCH §2.2, §3.4): the shipped verb drives
//! the authoring core through the `$EDITOR` hand-off. A scripted editor
//! stands in for the interactive one — it writes a file into the
//! authoring checkout it is handed — so the whole bin seam is exercised:
//! advancing the default branch, forking a new one, and starting a fresh
//! orphan lineage.

use std::fs;
use std::path::Path;
use std::process::Command;

fn litany_bin() -> &'static str {
    env!("CARGO_BIN_EXE_litany")
}

/// Git env vars a hook-invoked test may inherit; scrub them so the
/// spawned `git` operates on the fixture, not the outer repo (the same
/// discipline `litany`'s own `RealGit` applies).
const INHERITED_GIT_ENV: &[&str] = &[
    "GIT_DIR",
    "GIT_WORK_TREE",
    "GIT_INDEX_FILE",
    "GIT_OBJECT_DIRECTORY",
    "GIT_PREFIX",
    "GIT_COMMON_DIR",
    "GIT_ALTERNATE_OBJECT_DIRECTORIES",
];

fn git(dest: &Path, args: &[&str]) -> std::process::Output {
    let mut cmd = Command::new("git");
    for var in INHERITED_GIT_ENV {
        cmd.env_remove(var);
    }
    cmd.arg("-C")
        .arg(dest)
        .args(args)
        .output()
        .expect("spawn git")
}

/// A scripted `$EDITOR`: a shell script that writes `content` to `rel`
/// inside the directory it is invoked on (`"$1"`). Returns its path.
fn editor_writing(dir: &Path, name: &str, rel: &str, content: &str) -> std::path::PathBuf {
    let path = dir.join(name);
    fs::write(
        &path,
        format!("#!/bin/sh\nmkdir -p \"$(dirname \"$1/{rel}\")\"\nprintf '%s' '{content}' > \"$1/{rel}\"\n"),
    )
    .unwrap();
    let mut perms = fs::metadata(&path).unwrap().permissions();
    use std::os::unix::fs::PermissionsExt;
    perms.set_mode(0o755);
    fs::set_permissions(&path, perms).unwrap();
    path
}

fn run_config(ws: &Path, home: &Path, editor: &Path, extra: &[&str]) {
    let out = Command::new(litany_bin())
        .arg("config")
        .arg(ws)
        .args(extra)
        .env("LITANY_HOME", home)
        .env("EDITOR", editor)
        .output()
        .expect("spawn litany config");
    assert!(
        out.status.success(),
        "litany config {extra:?}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

/// `litany config` expected to decline: returns its stderr, asserting a
/// non-zero exit.
fn run_config_declining(ws: &Path, home: &Path, editor: &Path, extra: &[&str]) -> String {
    let out = Command::new(litany_bin())
        .arg("config")
        .arg(ws)
        .args(extra)
        .env("LITANY_HOME", home)
        .env("EDITOR", editor)
        .output()
        .expect("spawn litany config");
    assert!(
        !out.status.success(),
        "litany config {extra:?} must decline"
    );
    String::from_utf8_lossy(&out.stderr).trim().to_string()
}

fn new_workspace(ws: &Path, home: &Path) {
    let out = Command::new(litany_bin())
        .arg("new")
        .arg(ws)
        .env("LITANY_HOME", home)
        .output()
        .expect("spawn litany new");
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
}

#[test]
fn config_verb_advances_forks_and_orphans_via_editor() {
    let holder = tempfile::TempDir::new().unwrap();
    let home = holder.path().join("home");
    let ws = holder.path().join("ws");
    fs::create_dir_all(&home).unwrap();
    new_workspace(&ws, &home);
    let repo = ws.join("repo.git");

    // Advance config/default: the scripted editor rewrites providers.yaml.
    let ed = editor_writing(holder.path(), "adv.sh", "providers.yaml", "roles: {}\n");
    run_config(&ws, &home, &ed, &[]);
    let providers = git(&repo, &["show", "config/default:providers.yaml"]);
    assert_eq!(String::from_utf8_lossy(&providers.stdout), "roles: {}\n");

    // Fork config/strict off config/default.
    let ed = editor_writing(
        holder.path(),
        "fork.sh",
        "providers.yaml",
        "roles: {s: 1}\n",
    );
    run_config(&ws, &home, &ed, &["strict", "--from", "default"]);
    assert!(
        git(&repo, &["show", "config/strict:providers.yaml"])
            .status
            .success()
    );
    // The fork shares default's ancestry.
    assert!(
        git(&repo, &["merge-base", "config/default", "config/strict"])
            .status
            .success()
    );

    // Orphan config/scratch: a fresh lineage with no shared ancestor.
    let ed = editor_writing(holder.path(), "orphan.sh", "note.txt", "fresh\n");
    run_config(&ws, &home, &ed, &["scratch", "--orphan"]);
    assert!(
        git(&repo, &["show", "config/scratch:note.txt"])
            .status
            .success()
    );
    assert!(
        !git(&repo, &["merge-base", "config/default", "config/scratch"])
            .status
            .success(),
        "orphan must share no ancestry with config/default"
    );

    // `--from` a lineage that does not exist: the shipped verb names the
    // missing lineage and the pool that does exist, with no git plumbing,
    // no `.config-author`, and no `config/` ref prefix in the message.
    let stderr = run_config_declining(&ws, &home, &ed, &["x", "--from", "nosuch"]);
    assert_eq!(
        stderr,
        "litany config: no config lineage \"nosuch\" in this workspace — \
         existing lineages: default, scratch, strict"
    );
    assert!(!ws.join(".config-author").exists());
    assert!(
        !git(&repo, &["show", "config/x:providers.yaml"])
            .status
            .success()
    );
}
