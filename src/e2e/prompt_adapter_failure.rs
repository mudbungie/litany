//! End-to-end: an adapter that is missing, or that fails at startup,
//! says why (ARCH §2.3 step record, §4.4).
//!
//! **Absent.** The first real command of every user who did not install
//! from the repo — neither `cargo install litany` nor the release
//! tarball lays `bz` down. Asserted through the real binary on a `PATH`
//! carrying `git` and no `bz` at all: the refusal must name the adapter,
//! the pin, and the command that installs it, with the errno trailing.
//!
//! **Present but broken.** Real `bz`, pointed at a malformed brazen
//! config: it dies before it
//! can emit a single `v=1` event, so stdout is empty and the whole
//! complaint is on stderr. On disk that is indistinguishable from a
//! mid-stream kill (§2.9) — the operator-visible difference is the
//! captured stderr, quoted in the error and landed beside the empty
//! `response.json` as `stderr.log`.

use super::prompt_end_to_end::{scaffold_repo, write_global_models};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use tempfile::TempDir;

/// A `bin` directory holding `git` and nothing else, for use as a whole
/// `PATH`: the harness shells `git`, so a bare empty `PATH` would fail
/// for the wrong reason, and naming a system directory would risk
/// picking up whatever `bz` the machine happens to carry. `git` is
/// located by walking the live `PATH` — the same lookup the child would
/// have done — and symlinked in.
fn path_without_bz(holder: &Path) -> PathBuf {
    let bin = holder.join("bin");
    fs::create_dir_all(&bin).unwrap();
    let git = std::env::split_paths(&std::env::var_os("PATH").unwrap())
        .map(|d| d.join("git"))
        .find(|p| p.is_file())
        .expect("git on PATH");
    std::os::unix::fs::symlink(git, bin.join("git")).unwrap();
    bin
}

#[test]
fn a_missing_bz_names_the_adapter_the_pin_and_the_install_command() {
    let holder = TempDir::new().unwrap();
    let harness = holder.path().join("harness");
    fs::create_dir_all(&harness).unwrap();
    write_global_models(&harness);
    let dest = holder.path().join("conv");
    scaffold_repo(&dest, &harness);

    let out = Command::new(crate::test_support::litany_binary())
        .arg("prompt")
        .arg(&dest)
        .arg("ping")
        .env("LITANY_HOME", &harness)
        .env("PATH", path_without_bz(holder.path()))
        .output()
        .expect("spawn litany prompt");
    assert_eq!(out.status.code(), Some(1));
    let stderr = String::from_utf8_lossy(&out.stderr);
    // The whole thread back to the fix, in the version guard's voice:
    // the verb prefix, the binary, the section, and the command — with
    // the errno trailing as detail rather than standing alone.
    assert!(
        stderr.contains("litany prompt: provider adapter \"bz\" not found (ARCH §4.4 —"),
        "{stderr}"
    );
    assert!(
        stderr.contains(&format!(
            "cargo install brazen --version ={} --locked",
            crate::prompt::brazen_pin()
        )),
        "{stderr}"
    );
    assert!(stderr.contains("No such file or directory"), "{stderr}");
}

#[test]
fn a_bz_that_dies_at_startup_surfaces_its_stderr() {
    let holder = TempDir::new().unwrap();
    let harness = holder.path().join("harness");
    fs::create_dir_all(&harness).unwrap();
    write_global_models(&harness);
    let dest = holder.path().join("conv");
    scaffold_repo(&dest, &harness);

    // Not TOML. `bz --version` (the load-time guard, §4.4) does not read
    // the config, so the failure lands where it hurts: mid-model-call.
    let brazen_config = holder.path().join("brazen.toml");
    fs::write(&brazen_config, "this is not = valid toml [[[\n").unwrap();

    let out = Command::new(crate::test_support::litany_binary())
        .arg("prompt")
        .arg(&dest)
        .arg("ping")
        .env("LITANY_HOME", &harness)
        .env("BRAZEN_CONFIG", &brazen_config)
        .output()
        .expect("spawn litany prompt");
    assert!(!out.status.success(), "a dead adapter is not a success");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("malformed config"), "{stderr}");
    assert!(stderr.contains("TOML parse error"), "{stderr}");
    assert!(stderr.contains("stderr.log"), "{stderr}");

    // The artifact holds the whole capture, beside the response that
    // never arrived (§2.3).
    let steps = dest.join("steps");
    let agent = fs::read_dir(&steps)
        .unwrap()
        .next()
        .unwrap()
        .unwrap()
        .path();
    let log = fs::read_to_string(agent.join("001/stderr.log")).unwrap();
    assert!(log.contains("expected `.`, `=`"), "{log}");
    assert!(
        fs::read(agent.join("001/response.json"))
            .unwrap()
            .is_empty(),
        "the adapter never reached the contract"
    );
}
