//! `make install` end-to-end: harness root scaffold (ARCH §2.2), the
//! global `models.yaml` (ARCH §4.2), and idempotency.
//!
//! The Makefile is the public install contract. This test pins its
//! observable shape so re-runs never clobber hand-edited config and the
//! layout matches what the runtime resolvers expect. The provider
//! adapter is brazen's `bz`, installed by `cargo install brazen` onto
//! the user's cargo bin (§4.4) — not into the harness root — so this
//! test asserts the harness-owned layout only, and redirects that one
//! write away from the user's cargo bin (see `run_install`).

// Tarpaulin sets `--cfg=tarpaulin` at compile time; the test below uses
// `cfg_attr(tarpaulin, ignore)` to skip itself under instrumented runs.
#![allow(unexpected_cfgs)]

use std::path::{Path, PathBuf};
use std::process::Command;
use tempfile::TempDir;

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

/// Where this test lets `make install`'s `cargo install brazen` land.
///
/// `install-bz` (the Makefile) installs the pinned `bz` with no `--root`,
/// so cargo's default root — the user's `CARGO_HOME`, i.e.
/// `~/.cargo/bin/bz` — is a machine-global singleton. A test may not
/// write it: sibling worktrees at different `brazen` pins would take
/// turns rolling each other's `bz` back, and the load-time version guard
/// (§4.4) then fails the OTHER worktree's e2e gate with what reads like a
/// code regression. So the test redirects the write with cargo's own
/// `CARGO_INSTALL_ROOT` (its documented root override, ahead of
/// `CARGO_HOME` and behind an explicit `--root`) — no test-only branch in
/// the recipe, and `make install` run by a user is unchanged.
///
/// The root is per-worktree and persistent rather than a `TempDir`, so
/// cargo's "already installed" short-circuit keeps re-runs free; it lives
/// under `target/`, which is already this tree's scratch space.
fn bz_install_root() -> PathBuf {
    repo_root().join("target/install-test-cargo-root")
}

fn run_install(prefix: &Path, home: &Path) {
    let out = Command::new("make")
        .current_dir(repo_root())
        .arg("install")
        .arg(format!("INSTALL_PREFIX={}", prefix.display()))
        .arg(format!("LITANY_HOME={}", home.display()))
        .env("CARGO_INSTALL_ROOT", bz_install_root())
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
        .expect("invoke make install");
    assert!(
        out.status.success(),
        "make install failed (status {}):\nstdout: {}\nstderr: {}",
        out.status,
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr),
    );
}

// `make install` shells out to `cargo build --workspace --release` (and
// `cargo install brazen`), which contends with tarpaulin's `target/`
// lock. Skip under tarpaulin — the test only exercises shell glue, so
// excluding it from instrumented runs has no effect on Rust line
// coverage.
#[cfg_attr(tarpaulin, ignore)]
#[test]
fn make_install_lays_down_skeleton_idempotently() {
    let prefix = TempDir::new().unwrap();
    let home = TempDir::new().unwrap();
    run_install(prefix.path(), home.path());

    // Harness-root skeleton (ARCH §2.2). No `adapters/` — the adapter is
    // brazen's `bz` on PATH now (§4.4). No `agents/` profile pool and no
    // `conversations/` tree — the pool dissolved into config commits
    // (fork is the freeze, §2.2) and workspaces live under `workspaces/`.
    for d in ["workflows", "tools", "skills", "workspaces"] {
        assert!(
            home.path().join(d).is_dir(),
            "harness root subdir missing: {d}"
        );
    }
    assert!(
        !home.path().join("adapters").exists(),
        "the retired per-provider adapters/ dir must not be created"
    );
    assert!(
        !home.path().join("agents").exists(),
        "the retired frozen-copy agents/ profile pool must not be created (§2.2)"
    );

    // Path binaries land under INSTALL_PREFIX/bin.
    let bin = prefix.path().join("bin");
    for b in ["litany", "agent-eval", "litany-eval-agent"] {
        assert!(bin.join(b).is_file(), "{b} missing from bin/");
    }

    // Default global models.yaml (ARCH §4.2) — mechanism only (bl-35e2):
    // no models table, no model id, no auth material in the seed.
    let models = home.path().join("models.yaml");
    let body = std::fs::read_to_string(&models).unwrap();
    assert!(!body.contains("models:"), "no models table ships (bl-35e2)");
    assert!(!body.contains("claude-"), "no model id ships (bl-35e2)");
    assert!(
        !body.contains("ANTHROPIC_API_KEY"),
        "auth material must not live in models.yaml (§4.1)"
    );

    // Idempotency: hand-edit config, re-run, verify it survives.
    std::fs::write(&models, "adapter: /opt/bz\n").unwrap();

    run_install(prefix.path(), home.path());

    assert_eq!(
        std::fs::read_to_string(&models).unwrap(),
        "adapter: /opt/bz\n",
        "models.yaml was clobbered by re-install"
    );
}
