//! Failure-mode coverage for [`super::super::run`]: every variant of
//! [`super::super::Error`] gets its own targeted test so a coverage
//! regression points at the offending path.

use super::super::*;
use super::fixtures::{ErrSpawner, StubEnv, StubSpawner, env, fake_repo, input_for};
use crate::prompt::role::validate::Invalid;
use std::collections::HashMap;
use std::io::Cursor;
use tempfile::TempDir;

#[test]
fn invalid_input_json_surfaces_invalidjson() {
    let (_h, repo) = fake_repo("worker");
    let mut stdin = Cursor::new(b"not json".to_vec());
    let mut stdout = Vec::new();
    let env = env(&repo, "p1");
    let err = run(&mut stdin, &mut stdout, &env, &StubSpawner::ok("ignored")).unwrap_err();
    assert!(matches!(err, Error::InvalidJson(_)), "{err}");
}

#[test]
fn missing_role_field_surfaces_invalidjson() {
    let (_h, repo) = fake_repo("worker");
    let mut stdin = Cursor::new(br#"{"goal":"g"}"#.to_vec());
    let mut stdout = Vec::new();
    let env = env(&repo, "p1");
    let err = run(&mut stdin, &mut stdout, &env, &StubSpawner::ok("ignored")).unwrap_err();
    assert!(matches!(err, Error::InvalidJson(_)), "{err}");
}

#[test]
fn unknown_input_field_surfaces_invalidjson() {
    let (_h, repo) = fake_repo("worker");
    let mut stdin = Cursor::new(br#"{"role":"worker","goal":"g","extra":"bad"}"#.to_vec());
    let mut stdout = Vec::new();
    let env = env(&repo, "p1");
    let err = run(&mut stdin, &mut stdout, &env, &StubSpawner::ok("ignored")).unwrap_err();
    assert!(matches!(err, Error::InvalidJson(_)), "{err}");
}

#[test]
fn missing_conv_repo_env_surfaces_missingenv() {
    let mut stdin = Cursor::new(input_for("worker", "g"));
    let mut stdout = Vec::new();
    let mut m = HashMap::new();
    m.insert(crate::prompt::tool::ENV_CONV_BRANCH, OsString::from("p1"));
    let env = StubEnv(m);
    let err = run(&mut stdin, &mut stdout, &env, &StubSpawner::ok("ignored")).unwrap_err();
    match err {
        Error::MissingEnv(name) => assert_eq!(name, "LITANY_CONV_REPO"),
        other => panic!("expected MissingEnv, got {other}"),
    }
}

#[test]
fn missing_conv_branch_env_surfaces_missingenv() {
    let repo = TempDir::new().unwrap();
    let mut stdin = Cursor::new(input_for("worker", "g"));
    let mut stdout = Vec::new();
    let mut m = HashMap::new();
    m.insert(
        crate::prompt::tool::ENV_CONV_REPO,
        repo.path().as_os_str().to_owned(),
    );
    let env = StubEnv(m);
    let err = run(&mut stdin, &mut stdout, &env, &StubSpawner::ok("ignored")).unwrap_err();
    match err {
        Error::MissingEnv(name) => assert_eq!(name, "LITANY_CONV_BRANCH"),
        other => panic!("expected MissingEnv, got {other}"),
    }
}

#[test]
fn non_utf8_branch_env_surfaces_missingenv() {
    use std::os::unix::ffi::OsStringExt;
    let repo = TempDir::new().unwrap();
    let mut stdin = Cursor::new(input_for("worker", "g"));
    let mut stdout = Vec::new();
    let mut m = HashMap::new();
    m.insert(
        crate::prompt::tool::ENV_CONV_REPO,
        repo.path().as_os_str().to_owned(),
    );
    m.insert(
        crate::prompt::tool::ENV_CONV_BRANCH,
        OsString::from_vec(vec![0xff, 0xff]),
    );
    let env = StubEnv(m);
    let err = run(&mut stdin, &mut stdout, &env, &StubSpawner::ok("ignored")).unwrap_err();
    match err {
        Error::MissingEnv(name) => assert_eq!(name, "LITANY_CONV_BRANCH"),
        other => panic!("expected MissingEnv, got {other}"),
    }
}

#[test]
fn missing_workspace_surfaces_governing_config_error() {
    // A bare directory has no workspace repository: the governing-config
    // derivation for the calling branch fails loudly (§2.2).
    let repo = TempDir::new().unwrap();
    let mut stdin = Cursor::new(input_for("worker", "g"));
    let mut stdout = Vec::new();
    let env = env(repo.path(), "p1");
    let err = run(&mut stdin, &mut stdout, &env, &StubSpawner::ok("ignored")).unwrap_err();
    assert!(
        matches!(err, Error::Role(Invalid::Governing { .. })),
        "{err}"
    );
}

#[test]
fn malformed_providers_yaml_surfaces_config_error() {
    // A config commit whose providers.yaml carries a legacy block is a
    // hard load error (§4.1) — surfaced through the Config variant.
    let (_h, repo) = fake_repo("worker");
    crate::workspace::fixture::amend_config(&repo, &[("providers.yaml", "providers: {}\n")]);
    crate::workspace::fixture::spawn_root(&repo, "p9");
    let mut stdin = Cursor::new(input_for("worker", "g"));
    let mut stdout = Vec::new();
    let env = env(&repo, "p9");
    let err = run(&mut stdin, &mut stdout, &env, &StubSpawner::ok("ignored")).unwrap_err();
    assert!(matches!(err, Error::Role(Invalid::Config(_))), "{err}");
}

#[test]
fn unknown_role_surfaces_rolemissing() {
    let (_h, repo) = fake_repo("worker");
    let mut stdin = Cursor::new(input_for("verifier", "g"));
    let mut stdout = Vec::new();
    let env = env(&repo, "p1");
    let err = run(&mut stdin, &mut stdout, &env, &StubSpawner::ok("ignored")).unwrap_err();
    match err {
        Error::Role(Invalid::RoleMissing { ref role, .. }) => assert_eq!(role, "verifier"),
        other => panic!("expected RoleMissing, got {other}"),
    }
}

#[test]
fn role_listed_but_soul_missing_surfaces_soulmissing() {
    // A config listing the role but carrying no soul for it: the
    // presence test against the config commit's tree fails (§4.3).
    let (_h, repo) = fake_repo("worker");
    let yaml = "roles:\n  verifier:\n    provider: anthropic\n    model: sonnet\n";
    crate::workspace::fixture::amend_config(&repo, &[("providers.yaml", yaml)]);
    crate::workspace::fixture::spawn_root(&repo, "p9");
    let mut stdin = Cursor::new(input_for("verifier", "g"));
    let mut stdout = Vec::new();
    let env = env(&repo, "p9");
    let err = run(&mut stdin, &mut stdout, &env, &StubSpawner::ok("ignored")).unwrap_err();
    match err {
        Error::Role(Invalid::SoulMissing {
            ref role,
            ref agent,
        }) => {
            assert_eq!(role, "verifier");
            assert_eq!(agent, "p9");
        }
        other => panic!("expected SoulMissing, got {other}"),
    }
}

#[test]
fn spawn_io_error_surfaces_spawn() {
    let (_h, repo) = fake_repo("worker");
    let mut stdin = Cursor::new(input_for("worker", "g"));
    let mut stdout = Vec::new();
    let env = env(&repo, "p1");
    let err = run(&mut stdin, &mut stdout, &env, &ErrSpawner).unwrap_err();
    match err {
        Error::Spawn { role, .. } => assert_eq!(role, "worker"),
        other => panic!("expected Spawn, got {other}"),
    }
}

#[test]
fn nonzero_exit_surfaces_dispatchexit() {
    let (_h, repo) = fake_repo("worker");
    let mut stdin = Cursor::new(input_for("worker", "g"));
    let mut stdout = Vec::new();
    let env = env(&repo, "p1");
    let spawner = StubSpawner::failing("kaboom", 7);
    let err = run(&mut stdin, &mut stdout, &env, &spawner).unwrap_err();
    match err {
        Error::DispatchExit { exit, stderr, .. } => {
            assert_eq!(exit, 7);
            assert_eq!(stderr, "kaboom");
        }
        other => panic!("expected DispatchExit, got {other}"),
    }
}

#[test]
fn empty_stdout_surfaces_emptyhandle() {
    let (_h, repo) = fake_repo("worker");
    let mut stdin = Cursor::new(input_for("worker", "g"));
    let mut stdout = Vec::new();
    let env = env(&repo, "p1");
    let err = run(&mut stdin, &mut stdout, &env, &StubSpawner::empty_stdout()).unwrap_err();
    assert!(matches!(err, Error::EmptyHandle { .. }), "{err}");
}

#[test]
fn write_failure_on_stdout_surfaces_write() {
    struct BrokenStdout;
    impl std::io::Write for BrokenStdout {
        fn write(&mut self, _buf: &[u8]) -> io::Result<usize> {
            Err(io::Error::from(io::ErrorKind::BrokenPipe))
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }
    let (_h, repo) = fake_repo("worker");
    let mut stdin = Cursor::new(input_for("worker", "g"));
    let mut stdout = BrokenStdout;
    let env = env(&repo, "p1");
    let err = run(&mut stdin, &mut stdout, &env, &StubSpawner::ok("p1-sub")).unwrap_err();
    assert!(matches!(err, Error::Write(_)), "{err}");
}

#[test]
fn stdin_read_failure_surfaces_stdinread() {
    struct BrokenStdin;
    impl std::io::Read for BrokenStdin {
        fn read(&mut self, _buf: &mut [u8]) -> io::Result<usize> {
            Err(io::Error::from(io::ErrorKind::ConnectionReset))
        }
    }
    let (_h, repo) = fake_repo("worker");
    let mut stdin = BrokenStdin;
    let mut stdout = Vec::new();
    let env = env(&repo, "p1");
    let err = run(&mut stdin, &mut stdout, &env, &StubSpawner::ok("ignored")).unwrap_err();
    assert!(matches!(err, Error::StdinRead(_)), "{err}");
}
