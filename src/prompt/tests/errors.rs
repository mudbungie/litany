//! Pre-branch and adapter error paths for [`crate::prompt::run`].
//!
//! Covers failures the harness surfaces before the branch is spawned
//! (config, role, version guard, soul, workflow) and after the model
//! call returns (in-band `Error` events). Disk/git failures during
//! branch work live in [`super::errors_disk`].

use super::fixtures::*;
use crate::prompt::Error;
use brazen::ErrorKind;
use serde_json::Value;
use std::io;
use std::path::{Path, PathBuf};
use tempfile::TempDir;

/// Drive `run` with default stubs against an explicit config root.
fn run_with_harness(
    repo: &Path,
    msg: &str,
    adapter: &StubAdapter,
    git: &StubGit,
    config_root: &Path,
) -> Result<String, Error> {
    let clock = FixedClock::default();
    let id = FixedIdGen;
    let (sleeper, tool_executor) = (StubSleeper::default(), StubToolExecutor::ok());
    crate::prompt::run(
        repo,
        msg,
        None,
        None,
        None,
        crate::prompt::PinnedDocs::none(),
        None,
        None,
        &valid_deps(
            adapter,
            &sleeper,
            git,
            &clock,
            &id,
            &tool_executor,
            config_root,
        ),
    )
}

#[test]
fn run_surfaces_global_models_yaml_load_error() {
    let repo = scaffold_repo(VALID_PER_REPO_PROVIDERS_YAML, Some("body"));
    let empty_harness = TempDir::new().unwrap();
    let err = run_with_harness(
        repo.path(),
        "hi",
        &unreachable_adapter(),
        &StubGit::ok(),
        empty_harness.path(),
    )
    .unwrap_err();
    assert!(matches!(err, Error::Config(_)), "got {err:?}");
}

#[test]
fn run_rejects_when_worker_role_missing() {
    let no_worker = r#"
roles:
  compactor:
    provider: anthropic
    model: claude-sonnet-5
"#;
    let repo = scaffold_repo(no_worker, Some("body"));
    let err =
        run_with_stubs(repo.path(), "hi", &unreachable_adapter(), &StubGit::ok()).unwrap_err();
    match err {
        Error::RoleMissing(role) => assert_eq!(role, "worker"),
        other => panic!("expected RoleMissing, got {other:?}"),
    }
}

#[test]
fn run_surfaces_version_skew() {
    // The version guard runs before any branch work: a `bz` reporting a
    // version other than the linked crate pin is declined (§4.4).
    let repo = scaffold_repo(VALID_PER_REPO_PROVIDERS_YAML, Some("body"));
    let adapter = StubAdapter::scripted([StubAdapter::reply_ok(b"bz 9.9.9\n")]);
    let git = StubGit::ok();
    let err = run_with_stubs(repo.path(), "hi", &adapter, &git).unwrap_err();
    match err {
        Error::VersionSkew { found, .. } => assert_eq!(found, "9.9.9"),
        other => panic!("expected VersionSkew, got {other:?}"),
    }
    // Only control-read git (rev-parse + show) preceded the guard —
    // nothing mutating (no worktree add) before it passes (§4.4).
    assert!(
        git.runs.borrow().iter().all(|(_, a)| a[0] != "worktree"),
        "no branch work before the guard passes"
    );
}

#[test]
fn run_names_a_missing_adapter_and_the_command_that_installs_it() {
    // The first real command of every binary-install user: no `bz` on
    // PATH. The refusal must carry what the version guard carries —
    // the binary, the section, and the literal fix-it command at the
    // linked pin — with the errno as trailing detail, not the headline.
    let repo = scaffold_repo(VALID_PER_REPO_PROVIDERS_YAML, Some("body"));
    let adapter = StubAdapter::scripted([StubAdapter::reply_err(io::ErrorKind::NotFound, "no bz")]);
    let git = StubGit::ok();
    let err = run_with_stubs(repo.path(), "hi", &adapter, &git).unwrap_err();
    assert!(matches!(err, Error::AdapterMissing { .. }), "{err}");
    let s = err.to_string();
    assert!(
        s.starts_with("provider adapter \"bz\" not found (ARCH §4.4 —"),
        "{s}"
    );
    assert!(
        s.contains(&format!(
            "cargo install brazen --version ={} --locked",
            crate::prompt::brazen_pin()
        )),
        "{s}"
    );
    assert!(s.ends_with("): no bz"), "the errno trails as detail: {s}");
    assert!(git.runs.borrow().iter().all(|(_, a)| a[0] != "worktree"));
}

#[test]
fn run_surfaces_model_call_spawn_failure() {
    // Version ok, branch spawned, dispatch committed; the model-call
    // `bz` fails at spawn.
    let repo = scaffold_repo(VALID_PER_REPO_PROVIDERS_YAML, Some("body"));
    let adapter = StubAdapter::scripted([
        StubAdapter::reply_ok(&version_line()),
        StubAdapter::reply_err(io::ErrorKind::BrokenPipe, "model call crashed"),
    ]);
    let err = run_with_stubs(repo.path(), "hi", &adapter, &StubGit::ok()).unwrap_err();
    assert!(matches!(err, Error::AdapterSpawn(_)));
}

#[test]
fn run_retries_on_retryable_error_then_completes() {
    // End-to-end retry through `run` (§2.10): a retryable 529 on the
    // first attempt, a clean stream on the second. The harness sleeps
    // the backoff (StubSleeper records it) and `response.json` carries
    // two attempt segments; the branch completes and merges.
    let repo = scaffold_repo(VALID_PER_REPO_PROVIDERS_YAML, Some("body"));
    let harness = scaffold_harness_root();
    let adapter = StubAdapter::scripted([
        StubAdapter::reply_ok(&version_line()),
        StubAdapter::reply_ok(&error_stream(
            ErrorKind::Provider { status: 529 },
            "overloaded",
        )),
        StubAdapter::reply_ok(&happy_response_bytes()),
    ]);
    let git = StubGit::ok();
    let (clock, id) = (FixedClock::default(), FixedIdGen);
    let (sleeper, tool_executor) = (StubSleeper::default(), StubToolExecutor::ok());
    crate::prompt::run(
        repo.path(),
        "hi",
        None,
        None,
        None,
        crate::prompt::PinnedDocs::none(),
        None,
        None,
        &valid_deps(
            &adapter,
            &sleeper,
            &git,
            &clock,
            &id,
            &tool_executor,
            harness.path(),
        ),
    )
    .unwrap();
    // Exactly one backoff sleep drove the single retry.
    assert_eq!(sleeper.slept.borrow().len(), 1);
    // Two attempt segments on disk (two terminal `end` lines).
    let resp = std::fs::read(repo.path().join("steps/ct-1-deadbeef/001/response.json")).unwrap();
    let ends = resp
        .split(|b| *b == b'\n')
        .filter(|l| *l == br#"{"type":"end"}"#)
        .count();
    assert_eq!(ends, 2);
}

#[test]
fn run_surfaces_adapter_returning_in_band_error() {
    // A non-retryable in-band `Error` (auth) aborts the step (§2.10), and
    // the decline names the provider row the fixture's role is bound to
    // plus the command that credentials it (bl-7e9e).
    let repo = scaffold_repo(VALID_PER_REPO_PROVIDERS_YAML, Some("body"));
    let adapter = StubAdapter::happy(&error_stream(ErrorKind::Auth, "unauthorized"));
    let err = run_with_stubs(repo.path(), "hi", &adapter, &StubGit::ok()).unwrap_err();
    match err {
        Error::AdapterAuth {
            ref row,
            ref message,
        } => {
            assert_eq!(row, "anthropic");
            assert_eq!(message, "unauthorized");
        }
        other => panic!("expected AdapterAuth, got {other:?}"),
    }
    let rendered = err.to_string();
    assert!(
        rendered.contains("bz --login --provider anthropic"),
        "the decline states the fix: {rendered}"
    );
}

#[test]
fn run_surfaces_a_non_auth_in_band_error_naming_the_row() {
    // Every other in-band failure keeps the classification — and gains
    // the provider row, which brazen cannot supply (§4.3, bl-7e9e).
    let repo = scaffold_repo(VALID_PER_REPO_PROVIDERS_YAML, Some("body"));
    let adapter = StubAdapter::happy(&error_stream(ErrorKind::ParseInput, "bad request"));
    let err = run_with_stubs(repo.path(), "hi", &adapter, &StubGit::ok()).unwrap_err();
    match err {
        Error::AdapterError {
            ref kind, ref row, ..
        } => {
            assert_eq!(kind, "ParseInput");
            assert_eq!(row, "anthropic");
        }
        other => panic!("expected AdapterError, got {other:?}"),
    }
    assert!(
        err.to_string()
            .contains("provider error (ParseInput) on provider row \"anthropic\""),
        "{err}"
    );
}

// Stream-shape error paths (half-stream, malformed events) live in
// [`super::errors_stream`]; parse-shape paths in [`super::errors_parse`].

#[test]
fn error_display_includes_context() {
    // Exercises every `#[error("...")]` format line.
    let _: String = Error::RoleMissing("worker".into()).to_string();
    let _: String = Error::AdapterError {
        kind: "Usage".into(),
        row: "anthropic".into(),
        message: "m".into(),
    }
    .to_string();
    let _: String = Error::AdapterAuth {
        row: "anthropic".into(),
        message: "m".into(),
    }
    .to_string();
    let _: String = Error::AdapterHalfStream {
        stderr_log: "steps/c/001/stderr.log".into(),
        tail: "(empty)".into(),
    }
    .to_string();
    let _: String = Error::VersionSkew {
        found: "9.9.9".into(),
        expected: "0.0.2".into(),
    }
    .to_string();
    let _: String = Error::HandshakeMismatch {
        found: Some(2),
        expected: 1,
    }
    .to_string();
    let _: String = Error::ControlRead {
        path: PathBuf::from("/x"),
        source: io::Error::other("y"),
    }
    .to_string();
    let _: String = Error::AdapterSpawn(io::Error::other("x")).to_string();
    let _: String = Error::AdapterJson(serde_json::from_str::<Value>("{").unwrap_err()).to_string();
    let _: String = Error::Git {
        op: "add",
        source: io::Error::other("x"),
    }
    .to_string();
    let _: String = Error::Io(io::Error::other("x")).to_string();
    let _: String = Error::ToolExec {
        tool: "bash".into(),
        source: crate::prompt::ExecError::KilledBySignal {
            name: "bash".into(),
            signal: 11,
        },
    }
    .to_string();
    let load_err: crate::config::LoadError = crate::config::LoadError::UnresolvedRef {
        key: "k".into(),
        message: "m".into(),
    };
    let e: Error = load_err.into();
    let _: String = e.to_string();
}
