//! `<config-root>/template/` override tests (ARCH §2.2): the seed set
//! is the union of the embedded [`TEMPLATE`] with any same-named file
//! under the override dir winning, extra files included, and an absent
//! dir yielding exactly the embedded template. Split from
//! [`super::tests`] for the per-file line cap.

use super::{GitRunner, ScaffoldError, TEMPLATE, TEMPLATE_OVERRIDE_DIR, scaffold};
use crate::harness_root::Roots;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::process::Command;
use tempfile::TempDir;

/// Succeeds at every git step without touching a real repo, so the
/// authoring checkout's contents stay observable after `scaffold`.
struct NoopGit;

impl GitRunner for NoopGit {
    fn run(&self, _dest: &Path, _args: &[&str]) -> io::Result<()> {
        Ok(())
    }
    fn run_capture(&self, dest: &Path, args: &[&str]) -> io::Result<String> {
        self.run(dest, args).map(|_| String::new())
    }
}

#[test]
fn noop_run_capture_delegates_to_run() {
    // `scaffold` only calls `run`; the trait's other method is covered
    // here directly (the same shape as tests.rs's StubGit check).
    assert_eq!(NoopGit.run_capture(Path::new("."), &["x"]).unwrap(), "");
}

/// The embedded template's content for `rel` — the fallback half of the
/// seed-set union.
fn embedded(rel: &str) -> &'static str {
    std::str::from_utf8(TEMPLATE.get_file(rel).unwrap().contents()).unwrap()
}

struct Case {
    name: &'static str,
    /// `(relative path, content)` files authored under the override dir.
    overrides: &'static [(&'static str, &'static str)],
    /// Seeded files that must carry the override's literal content.
    expect_exact: &'static [(&'static str, &'static str)],
    /// Seeded files that must carry the embedded template's content.
    expect_embedded: &'static [&'static str],
}

const CASES: &[Case] = &[
    Case {
        // A same-named override file wins over the embedded one; the
        // untouched siblings keep their embedded content.
        name: "override_wins_embedded_fallback",
        overrides: &[("providers.yaml", "roles: {}\n")],
        expect_exact: &[("providers.yaml", "roles: {}\n")],
        expect_embedded: &["workflow.yaml", "manifest.yaml", "version"],
    },
    Case {
        // An extra file with no embedded counterpart is included.
        name: "extra_file_included",
        overrides: &[("extra.yaml", "x: 1\n")],
        expect_exact: &[("extra.yaml", "x: 1\n")],
        expect_embedded: &["providers.yaml"],
    },
    Case {
        // The union recurses: a nested override (souls/) wins while its
        // embedded sibling survives.
        name: "nested_override_wins",
        overrides: &[("souls/worker.md", "# Overridden Worker\n")],
        expect_exact: &[("souls/worker.md", "# Overridden Worker\n")],
        expect_embedded: &["souls/compactor.md"],
    },
    Case {
        // Absent override dir = today's behavior exactly: every control
        // file is the embedded one.
        name: "absent_dir_is_embedded_exactly",
        overrides: &[],
        expect_exact: &[],
        expect_embedded: &[
            "providers.yaml",
            "workflow.yaml",
            "manifest.yaml",
            "version",
            "souls/worker.md",
            "souls/compactor.md",
        ],
    },
];

#[test]
fn seed_set_is_union_with_override_winning() {
    for case in CASES {
        let holder = TempDir::new().unwrap();
        let config = holder.path().join("conf");
        for (rel, content) in case.overrides {
            let p = config.join(TEMPLATE_OVERRIDE_DIR).join(rel);
            fs::create_dir_all(p.parent().unwrap()).unwrap();
            fs::write(&p, content).unwrap();
        }
        let roots = Roots {
            config,
            data: holder.path().join("no-pool"),
        };
        let dest = holder.path().join("ws");
        scaffold(&dest, &roots, &NoopGit).unwrap();
        let author = dest.join(".config-author");
        for (rel, want) in case.expect_exact {
            let got = fs::read_to_string(author.join(rel)).unwrap();
            assert_eq!(&got, want, "{}: {rel}", case.name);
        }
        for rel in case.expect_embedded {
            let got = fs::read_to_string(author.join(rel)).unwrap();
            assert_eq!(got, embedded(rel), "{}: {rel}", case.name);
        }
    }
}

#[test]
fn override_path_that_is_a_file_surfaces_io() {
    // read_dir on a regular file fails with a non-NotFound kind — the
    // overlay's error arm, mapped to ScaffoldError::Io.
    let holder = TempDir::new().unwrap();
    let config = holder.path().join("conf");
    fs::create_dir_all(&config).unwrap();
    fs::write(config.join(TEMPLATE_OVERRIDE_DIR), b"not a dir").unwrap();
    let roots = Roots {
        config,
        data: holder.path().join("no-pool"),
    };
    let err = scaffold(&holder.path().join("ws"), &roots, &NoopGit).unwrap_err();
    assert!(matches!(err, ScaffoldError::Io(_)), "got {err:?}");
}

// --- LITANY_HOME collapse, end to end through the binary -----------

/// Git env vars a hook-invoked test may inherit; scrubbed so
/// subcommands operate on the tempdir, not the outer repo.
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

/// `git show config/default:<path>` against the workspace's bare repo.
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

#[test]
fn litany_home_collapse_reads_override_from_litany_home_template() {
    // With LITANY_HOME set the config root collapses to it (ARCH §2.2),
    // so the override lives at $LITANY_HOME/template/ — the nested-world
    // shape. The motivating case (bl-e795): a host whose only credential
    // is codex overrides providers.yaml so workspaces are born usable.
    let over_providers = "roles:\n  worker:\n    provider: codex\n    model: gpt-5.4\n";
    let home = TempDir::new().unwrap();
    let tmpl = home.path().join(TEMPLATE_OVERRIDE_DIR);
    fs::create_dir_all(&tmpl).unwrap();
    fs::write(tmpl.join("providers.yaml"), over_providers).unwrap();
    fs::write(tmpl.join("extra.txt"), "extra\n").unwrap();

    let holder = TempDir::new().unwrap();
    let dest: PathBuf = holder.path().join("ws");
    let mut cmd = Command::new(crate::test_support::litany_binary());
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
        "litany new failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    // The config commit carries the override, the extra file, and the
    // embedded fallback for everything untouched.
    assert_eq!(show_control(&dest, "providers.yaml"), over_providers);
    assert_eq!(show_control(&dest, "extra.txt"), "extra\n");
    assert_eq!(
        show_control(&dest, "workflow.yaml"),
        embedded("workflow.yaml")
    );
}
