//! Test-only helpers shared across the crate's in-crate integration
//! tests — the ones migrated in from `tests/` when the library surface
//! was narrowed to [`crate::cmd`] (§3.4). Not part of the public surface:
//! `#[cfg(test)]` and non-`pub` at the crate root, so the parity checker
//! (which counts only externally-public items) never sees it.

use crate::harness_root::Roots;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

/// Serializes `LITANY_HOME` mutation. The harness-root env (§2.2) is
/// process-global; every in-process test that must point it at a scratch
/// dir (`prime`/`new`, which found real files; `config`, whose
/// `harness_root::resolve` call is unmocked; `archive::replay_cli`'s
/// `LITANY_HOME`-scoped scratch base) funnels through [`with_litany_home`]
/// so the mutation is the *one* place a parallel `cargo test --lib` run
/// can race — the lock, not the caller's own scope, is what makes it
/// safe. Rust 2024's `set_var` is `unsafe` for exactly this reason.
static ENV_LOCK: Mutex<()> = Mutex::new(());

/// Run `f` with `LITANY_HOME` set to `home`, then clear it. Serialized
/// against every other `LITANY_HOME` mutation via [`ENV_LOCK`] — the
/// single guarded critical section, so a reader of the ambient env (e.g.
/// `harness_root::resolve` called from inside `f`) never observes another
/// test's home mid-flight. Tests never pre-set the var, so the restore is
/// an unconditional clear — not a save/restore of a prior value.
pub fn with_litany_home<R>(home: &Path, f: impl FnOnce() -> R) -> R {
    let _guard = ENV_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    // SAFETY: guarded by ENV_LOCK; no other thread reads/writes the var
    // concurrently while the guard is held.
    unsafe { std::env::set_var("LITANY_HOME", home) };
    let r = f();
    unsafe { std::env::remove_var("LITANY_HOME") };
    r
}

/// Run `f` with the §3.3 stdio contract's whole environment set —
/// `LITANY_HOME` plus `LITANY_CONV_REPO` / `LITANY_CONV_BRANCH` — then
/// clear all three. The `litany invoke` verb reads the contract vars
/// through [`crate::prompt::tool::builtin::dispatch::ProcessEnv`] like
/// every built-in, so driving the verb itself (rather than the door
/// beneath it) means setting them for real. Serialized against every
/// other mutation by the same [`ENV_LOCK`], for the same reason.
pub fn with_contract_env<R>(
    home: &Path,
    workspace: &Path,
    agent: &str,
    f: impl FnOnce() -> R,
) -> R {
    let _guard = ENV_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    // SAFETY: guarded by ENV_LOCK; no other thread reads/writes these
    // vars concurrently while the guard is held.
    unsafe {
        std::env::set_var("LITANY_HOME", home);
        std::env::set_var(crate::prompt::tool::ENV_CONV_REPO, workspace);
        std::env::set_var(crate::prompt::tool::ENV_CONV_BRANCH, agent);
    }
    let r = f();
    unsafe {
        std::env::remove_var("LITANY_HOME");
        std::env::remove_var(crate::prompt::tool::ENV_CONV_REPO);
        std::env::remove_var(crate::prompt::tool::ENV_CONV_BRANCH);
    }
    r
}

/// Roots pointing at nonexistent subdirs of `base` — no
/// `template/` override under the config root, no data-root pools:
/// the plain embedded-template scaffold shape most tests want.
pub fn bare_roots(base: &Path) -> Roots {
    Roots {
        config: base.join("no-conf"),
        data: base.join("no-pool"),
    }
}

/// Resolve the cargo-built `litany` binary from the running test binary.
///
/// `env!("CARGO_BIN_EXE_litany")` is set only for `tests/` integration
/// targets, not for lib unit tests, so an in-crate test that must spawn
/// the real binary derives it from `current_exe()`: the test executable
/// (`<target>/<profile>/deps/<test>-<hash>`) and the `litany` binary
/// (`<target>/<profile>/litany`) are siblings — walk up from the test
/// binary and take the first ancestor directory holding a `litany` file.
pub fn litany_binary() -> PathBuf {
    let test_exe = std::env::current_exe().expect("current_exe for the test binary");
    for dir in test_exe.ancestors() {
        let candidate = dir.join("litany");
        if candidate.is_file() {
            return candidate;
        }
    }
    panic!("built `litany` binary not found above {test_exe:?}; run `cargo build` first");
}
