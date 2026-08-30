//! Tests for the `message` built-in (ARCH §2.11). Every [`Error`]
//! variant gets a targeted test, plus the happy-path arg-forwarding
//! assertion and the production [`SubprocessSender`] smoke checks.

use super::*;
use std::cell::RefCell;
use std::collections::HashMap;
use std::io::Cursor;
use tempfile::TempDir;

/// HashMap-backed [`EnvLookup`] so tests pin `LITANY_CONV_REPO` /
/// `LITANY_CONV_BRANCH` without touching (racy) process env.
struct StubEnv(HashMap<&'static str, OsString>);
impl EnvLookup for StubEnv {
    fn get(&self, key: &str) -> Option<OsString> {
        self.0.get(key).cloned()
    }
}

fn env(repo: &Path, branch: &str) -> StubEnv {
    let mut m = HashMap::new();
    m.insert(ENV_CONV_REPO, repo.as_os_str().to_owned());
    m.insert(ENV_CONV_BRANCH, OsString::from(branch));
    StubEnv(m)
}

/// Recording [`Sender`] — captures the forwarded args and reports a
/// clean deposit.
#[derive(Default)]
struct StubSender {
    invocations: RefCell<Vec<(PathBuf, String, String, String)>>,
}
impl Sender for StubSender {
    fn send(
        &self,
        workspace: &Path,
        agent: &str,
        content: &str,
        sender: &str,
    ) -> Result<SendOutput, io::Error> {
        self.invocations.borrow_mut().push((
            workspace.to_path_buf(),
            agent.to_string(),
            content.to_string(),
            sender.to_string(),
        ));
        Ok(SendOutput {
            stderr: String::new(),
            exit: 0,
        })
    }
}

/// [`Sender`] that reports a non-zero exit with stderr.
struct FailSender {
    stderr: &'static str,
    exit: i32,
}
impl Sender for FailSender {
    fn send(&self, _w: &Path, _a: &str, _c: &str, _s: &str) -> Result<SendOutput, io::Error> {
        Ok(SendOutput {
            stderr: self.stderr.to_string(),
            exit: self.exit,
        })
    }
}

/// [`Sender`] that fails at the io layer — exercises [`Error::Spawn`].
struct ErrSender;
impl Sender for ErrSender {
    fn send(&self, _w: &Path, _a: &str, _c: &str, _s: &str) -> Result<SendOutput, io::Error> {
        Err(io::Error::new(io::ErrorKind::NotFound, "no litany binary"))
    }
}

fn input_for(agent: &str, content: &str) -> Vec<u8> {
    serde_json::json!({ "agent": agent, "content": content })
        .to_string()
        .into_bytes()
}

#[test]
fn happy_path_forwards_args_and_writes_deposited() {
    let repo = TempDir::new().unwrap();
    let mut stdin = Cursor::new(input_for("p1-child", "steer left"));
    let mut stdout = Vec::new();
    let env = env(repo.path(), "p1");
    let sender = StubSender::default();

    run(&mut stdin, &mut stdout, &env, &sender).unwrap();

    let payload: serde_json::Value = serde_json::from_slice(&stdout).unwrap();
    assert_eq!(payload["status"], "deposited");

    let invocations = sender.invocations.borrow();
    assert_eq!(invocations.len(), 1);
    assert_eq!(invocations[0].0, repo.path());
    assert_eq!(invocations[0].1, "p1-child");
    assert_eq!(invocations[0].2, "steer left");
    // Sender identity is the harness-set LITANY_CONV_BRANCH, not model
    // input — un-forgeable provenance (§2.11).
    assert_eq!(invocations[0].3, "p1");
}

#[test]
fn invalid_input_json_surfaces_invalidjson() {
    let repo = TempDir::new().unwrap();
    let mut stdin = Cursor::new(b"not json".to_vec());
    let mut stdout = Vec::new();
    let env = env(repo.path(), "p1");
    let err = run(&mut stdin, &mut stdout, &env, &StubSender::default()).unwrap_err();
    assert!(matches!(err, Error::InvalidJson(_)), "{err}");
}

#[test]
fn unknown_input_field_surfaces_invalidjson() {
    // deny_unknown_fields: a model cannot smuggle a `from` sender.
    let repo = TempDir::new().unwrap();
    let mut stdin = Cursor::new(br#"{"agent":"a","content":"c","from":"forged"}"#.to_vec());
    let mut stdout = Vec::new();
    let env = env(repo.path(), "p1");
    let err = run(&mut stdin, &mut stdout, &env, &StubSender::default()).unwrap_err();
    assert!(matches!(err, Error::InvalidJson(_)), "{err}");
}

#[test]
fn missing_conv_repo_env_surfaces_missingenv() {
    let mut stdin = Cursor::new(input_for("a", "c"));
    let mut stdout = Vec::new();
    let mut m = HashMap::new();
    m.insert(ENV_CONV_BRANCH, OsString::from("p1"));
    let env = StubEnv(m);
    let err = run(&mut stdin, &mut stdout, &env, &StubSender::default()).unwrap_err();
    match err {
        Error::MissingEnv(name) => assert_eq!(name, "LITANY_CONV_REPO"),
        other => panic!("expected MissingEnv, got {other}"),
    }
}

#[test]
fn missing_conv_branch_env_surfaces_missingenv() {
    let repo = TempDir::new().unwrap();
    let mut stdin = Cursor::new(input_for("a", "c"));
    let mut stdout = Vec::new();
    let mut m = HashMap::new();
    m.insert(ENV_CONV_REPO, repo.path().as_os_str().to_owned());
    let env = StubEnv(m);
    let err = run(&mut stdin, &mut stdout, &env, &StubSender::default()).unwrap_err();
    match err {
        Error::MissingEnv(name) => assert_eq!(name, "LITANY_CONV_BRANCH"),
        other => panic!("expected MissingEnv, got {other}"),
    }
}

#[test]
fn non_utf8_branch_env_surfaces_missingenv() {
    use std::os::unix::ffi::OsStringExt;
    let repo = TempDir::new().unwrap();
    let mut stdin = Cursor::new(input_for("a", "c"));
    let mut stdout = Vec::new();
    let mut m = HashMap::new();
    m.insert(ENV_CONV_REPO, repo.path().as_os_str().to_owned());
    m.insert(ENV_CONV_BRANCH, OsString::from_vec(vec![0xff, 0xff]));
    let env = StubEnv(m);
    let err = run(&mut stdin, &mut stdout, &env, &StubSender::default()).unwrap_err();
    match err {
        Error::MissingEnv(name) => assert_eq!(name, "LITANY_CONV_BRANCH"),
        other => panic!("expected MissingEnv, got {other}"),
    }
}

#[test]
fn spawn_io_error_surfaces_spawn() {
    let repo = TempDir::new().unwrap();
    let mut stdin = Cursor::new(input_for("a", "c"));
    let mut stdout = Vec::new();
    let env = env(repo.path(), "p1");
    let err = run(&mut stdin, &mut stdout, &env, &ErrSender).unwrap_err();
    assert!(matches!(err, Error::Spawn(_)), "{err}");
}

#[test]
fn nonzero_exit_surfaces_messageexit() {
    let repo = TempDir::new().unwrap();
    let mut stdin = Cursor::new(input_for("a", "c"));
    let mut stdout = Vec::new();
    let env = env(repo.path(), "p1");
    let sender = FailSender {
        stderr: "kaboom",
        exit: 5,
    };
    let err = run(&mut stdin, &mut stdout, &env, &sender).unwrap_err();
    match err {
        Error::MessageExit { exit, stderr } => {
            assert_eq!(exit, 5);
            assert_eq!(stderr, "kaboom");
        }
        other => panic!("expected MessageExit, got {other}"),
    }
}

#[test]
fn write_failure_on_stdout_surfaces_write() {
    struct BrokenStdout;
    impl Write for BrokenStdout {
        fn write(&mut self, _buf: &[u8]) -> io::Result<usize> {
            Err(io::Error::from(io::ErrorKind::BrokenPipe))
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }
    let repo = TempDir::new().unwrap();
    let mut stdin = Cursor::new(input_for("a", "c"));
    let mut stdout = BrokenStdout;
    let env = env(repo.path(), "p1");
    let err = run(&mut stdin, &mut stdout, &env, &StubSender::default()).unwrap_err();
    assert!(matches!(err, Error::Write(_)), "{err}");
}

#[test]
fn stdin_read_failure_surfaces_stdinread() {
    struct BrokenStdin;
    impl Read for BrokenStdin {
        fn read(&mut self, _buf: &mut [u8]) -> io::Result<usize> {
            Err(io::Error::from(io::ErrorKind::ConnectionReset))
        }
    }
    let repo = TempDir::new().unwrap();
    let mut stdin = BrokenStdin;
    let mut stdout = Vec::new();
    let env = env(repo.path(), "p1");
    let err = run(&mut stdin, &mut stdout, &env, &StubSender::default()).unwrap_err();
    assert!(matches!(err, Error::StdinRead(_)), "{err}");
}

#[test]
fn subprocess_sender_with_exe_captures_clean_exit() {
    // `true` exits 0 with empty stdio — the wrapper reports exit 0.
    let s = SubprocessSender::with_exe(PathBuf::from("true"));
    let out = s.send(Path::new("/tmp"), "a", "c", "p1").unwrap();
    assert_eq!(out.exit, 0);
    assert!(out.stderr.is_empty());
}

#[test]
fn subprocess_sender_with_exe_reports_nonzero() {
    // `false` exits 1 — the wrapper preserves the code.
    let s = SubprocessSender::with_exe(PathBuf::from("false"));
    let out = s.send(Path::new("/tmp"), "a", "c", "p1").unwrap();
    assert_eq!(out.exit, 1);
}

#[test]
fn subprocess_sender_with_exe_surfaces_missing_binary() {
    let s = SubprocessSender::with_exe(PathBuf::from("/no/such/litany-binary"));
    let err = s.send(Path::new("/tmp"), "a", "c", "p1").unwrap_err();
    assert_eq!(err.kind(), io::ErrorKind::NotFound);
}
