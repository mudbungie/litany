//! Descriptions-always producer, end-to-end through `litany new`
//! (ARCH §3.3). A populated data-root pool must produce a committed
//! `descriptions/**` tree in the workspace's first config commit
//! (`config/default`, §2.2), so a downstream branch's tools composer
//! (§3.3, bl-9e96) can intersect a role's declared tools against it.

use std::path::PathBuf;
use std::process::Command;
use tempfile::TempDir;

/// Git env vars a hook-invoked test may inherit; scrub them so the
/// spawned `git` operates on the scaffolded tempdir, not the outer repo.
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
    PathBuf::from(env!("CARGO_BIN_EXE_litany"))
}

#[test]
fn descriptions_are_snapshotted_from_the_pool_and_committed() {
    // A populated pool under LITANY_HOME (which collapses both roots):
    // one tool schema and one skill frontmatter.
    let home = TempDir::new().unwrap();
    let data = home.path();
    std::fs::create_dir_all(data.join("tools")).unwrap();
    std::fs::write(data.join("tools/bash.json"), r#"{"type":"object"}"#).unwrap();
    std::fs::create_dir_all(data.join("skills/bash")).unwrap();
    std::fs::write(
        data.join("skills/bash/SKILL.md"),
        "---\nname: bash\ndescription: Run a shell command.\n---\n# bash\n",
    )
    .unwrap();

    let holder = TempDir::new().unwrap();
    let dest = holder.path().join("conv");
    let mut cmd = Command::new(litany_bin());
    scrub_git_env(&mut cmd);
    let out = cmd
        .arg("new")
        .arg(&dest)
        .env("LITANY_HOME", data)
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
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );

    // The snapshot is committed in the config commit's tree (§2.2):
    // the tool schema verbatim, the skill file carrying only the
    // frontmatter body (fenced markdown stripped).
    let repo = dest.join("repo.git");
    let show = |path: &str| -> String {
        let mut cmd = Command::new("git");
        scrub_git_env(&mut cmd);
        let out = cmd
            .arg("-C")
            .arg(&repo)
            .args(["show", &format!("config/default:{path}")])
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "git show {path}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        String::from_utf8(out.stdout).unwrap()
    };
    assert_eq!(show("descriptions/tools/bash.json"), r#"{"type":"object"}"#);
    assert_eq!(
        show("descriptions/skills/bash.md"),
        "name: bash\ndescription: Run a shell command.\n"
    );
}

#[test]
fn an_unseeded_harness_root_is_founded_by_new_so_descriptions_are_never_empty() {
    // The papercut: `litany new` against a data root nobody primed used
    // to exit 0 with a config commit carrying no `descriptions/` at all,
    // so every agent forked off it saw an empty toolset (ARCH §3.3
    // descriptions-always). `new` now founds the root through prime's
    // own seed-if-absent routine (§2.2), so the unseeded root is not a
    // special case — it is the general path with empty inputs.
    let home = TempDir::new().unwrap();
    let holder = TempDir::new().unwrap();
    let dest = holder.path().join("ws");
    let mut cmd = Command::new(litany_bin());
    scrub_git_env(&mut cmd);
    let out = cmd
        .arg("new")
        .arg(&dest)
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
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );

    // The pools were founded under the previously-empty home …
    assert!(home.path().join("workspaces").is_dir());
    assert!(home.path().join("models.yaml").is_file());
    // … and every shipped tool rides the config commit's tree.
    let mut ls = Command::new("git");
    scrub_git_env(&mut ls);
    let listed = ls
        .arg("-C")
        .arg(dest.join("repo.git"))
        .args(["ls-tree", "-r", "--name-only", "config/default"])
        .output()
        .unwrap();
    let tree = String::from_utf8(listed.stdout).unwrap();
    for name in [
        "bash",
        "cd",
        "dispatch",
        "load_skill",
        "message",
        "read_file",
    ] {
        assert!(
            tree.contains(&format!("descriptions/tools/{name}.json")),
            "{name} schema missing from:\n{tree}"
        );
        assert!(
            tree.contains(&format!("descriptions/skills/{name}.md")),
            "{name} description missing from:\n{tree}"
        );
    }
    for control in [
        "manifest.yaml",
        "providers.yaml",
        "workflow.yaml",
        "version",
    ] {
        assert!(tree.contains(control), "{control} missing from:\n{tree}");
    }
}
