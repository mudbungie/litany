//! Tests for the dispatcher in [`super`]: each tool name routes to
//! its inner module, and inner errors surface through the matching
//! `Error` variant via `#[from]`.

use super::*;
use std::io::Cursor;
use std::path::Path;

/// [`super::run`] with the injected driver target (`cmd::Fx::driver_target`,
/// §2.11) supplied. No routing test here reaches the `dispatch` / `message`
/// arms that re-enter it — those are driven through [`super::run_with`] with
/// stub spawners — so a bare name suffices.
pub(super) fn route<R: Read, W: Write, E: Write>(
    name: &str,
    stdin: &mut R,
    stdout: &mut W,
    stderr: &mut E,
) -> Result<i32, Error> {
    let bindings = Bindings {
        driver_target: Path::new("litany"),
        adapter_target: None,
        stop: &std::sync::atomic::AtomicBool::new(false),
        injection: None,
    };
    run(name, &bindings, stdin, stdout, stderr)
}

/// The `bash`, `cd` and compactor-tool routing arms, and the advertised
/// pool, split out to keep this file under the repo's per-file line cap.
mod pool;
mod routing_apply_patch;
mod routing_bash;
mod routing_cd;
mod routing_compaction;
mod routing_python;
mod routing_search_history;

#[test]
fn read_file_routed_to_inner_module() {
    // A minimal-but-valid input that drives the inner module's
    // happy path. Exercising the dispatch arm for read_file.
    let tmp = tempfile::NamedTempFile::new().unwrap();
    std::fs::write(tmp.path(), b"hi").unwrap();
    let input = serde_json::json!({ "path": tmp.path() }).to_string();
    let mut stdin = Cursor::new(input.into_bytes());
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();
    let code = route("read_file", &mut stdin, &mut stdout, &mut stderr).unwrap();
    assert_eq!(code, 0);
    assert_eq!(stdout, b"hi");
}

#[test]
fn read_file_error_is_carried_through_dispatcher() {
    // Bad JSON on stdin — read_file::Error::InvalidJson — should
    // surface through the From conversion as Error::ReadFile.
    let mut stdin = Cursor::new(b"not json".to_vec());
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();
    let err = route("read_file", &mut stdin, &mut stdout, &mut stderr).unwrap_err();
    assert!(matches!(err, Error::ReadFile(_)), "{err}");
}

/// Test-only stub for the dispatch tool's [`Spawner`] dependency.
/// Returns a fixed handle on stdout, exit 0 — exercising the
/// happy-path arm of [`run_with`] without spawning a real
/// subprocess.
struct StubSpawner;
impl dispatch::Spawner for StubSpawner {
    fn dispatch(
        &self,
        _role: &str,
        _repo: &std::path::Path,
        _branch: &str,
        _goal: &str,
        _name: Option<&str>,
    ) -> std::io::Result<dispatch::DispatchOutput> {
        Ok(dispatch::DispatchOutput {
            stdout: "p1-sub\n".to_string(),
            stderr: String::new(),
            exit: 0,
        })
    }
}

/// Test-only stub for the message tool's [`message::Sender`]
/// dependency. Records nothing and reports a clean deposit — enough to
/// exercise the routing arm of [`run_with`] without spawning a real
/// `litany message` subprocess.
struct StubSender;
impl message::Sender for StubSender {
    fn send(
        &self,
        _workspace: &std::path::Path,
        _agent: &str,
        _content: &str,
        _sender: &str,
    ) -> std::io::Result<message::SendOutput> {
        Ok(message::SendOutput {
            stderr: String::new(),
            exit: 0,
        })
    }
}

/// HashMap-backed stub [`dispatch::EnvLookup`] — keyed by var name,
/// `None` for anything not seeded. Mirrors the dispatch module's own
/// test fixture so the dispatcher routing test does not invent a
/// second pattern.
struct StubEnv(std::collections::HashMap<&'static str, std::ffi::OsString>);
impl dispatch::EnvLookup for StubEnv {
    fn get(&self, key: &str) -> Option<std::ffi::OsString> {
        self.0.get(key).cloned()
    }
}

fn stub_env(repo: &std::path::Path, branch: &str) -> StubEnv {
    let mut m = std::collections::HashMap::new();
    m.insert(
        crate::prompt::tool::ENV_CONV_REPO,
        repo.as_os_str().to_owned(),
    );
    m.insert(
        crate::prompt::tool::ENV_CONV_BRANCH,
        std::ffi::OsString::from(branch),
    );
    StubEnv(m)
}

#[test]
fn dispatch_routed_to_inner_module() {
    let (_h, repo) = crate::workspace::fixture::workspace();
    crate::workspace::fixture::amend_config(
        &repo,
        &[(
            "providers.yaml",
            "roles:\n  worker:\n    provider: anthropic\n    model: m\n",
        )],
    );
    crate::workspace::fixture::spawn_root(&repo, "p1");

    let input = serde_json::json!({"role":"worker","goal":"g"}).to_string();
    let mut stdin = Cursor::new(input.into_bytes());
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();
    let env = stub_env(&repo, "p1");
    let code = run_with(
        "dispatch",
        &mut stdin,
        &mut stdout,
        &mut stderr,
        &env,
        &StubSpawner,
        &StubSender,
    )
    .unwrap();
    assert_eq!(code, 0);
    let payload: serde_json::Value = serde_json::from_slice(&stdout).unwrap();
    assert_eq!(payload["status"], "in_progress");
    assert_eq!(payload["handle"], "p1-sub");
}

#[test]
fn message_routed_to_inner_module() {
    let repo = tempfile::TempDir::new().unwrap();
    let input = serde_json::json!({"agent":"p1-child","content":"steer left"}).to_string();
    let mut stdin = Cursor::new(input.into_bytes());
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();
    let env = stub_env(repo.path(), "p1");
    let code = run_with(
        "message",
        &mut stdin,
        &mut stdout,
        &mut stderr,
        &env,
        &StubSpawner,
        &StubSender,
    )
    .unwrap();
    assert_eq!(code, 0);
    let payload: serde_json::Value = serde_json::from_slice(&stdout).unwrap();
    assert_eq!(payload["status"], "deposited");
}

#[test]
fn message_error_is_carried_through_dispatcher() {
    // No env vars — surfaces as message::Error::MissingEnv via #[from]
    // into Error::Message.
    struct EmptyEnv;
    impl dispatch::EnvLookup for EmptyEnv {
        fn get(&self, _key: &str) -> Option<std::ffi::OsString> {
            None
        }
    }
    let input = serde_json::json!({"agent":"p1-child","content":"hi"}).to_string();
    let mut stdin = Cursor::new(input.into_bytes());
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();
    let err = run_with(
        "message",
        &mut stdin,
        &mut stdout,
        &mut stderr,
        &EmptyEnv,
        &StubSpawner,
        &StubSender,
    )
    .unwrap_err();
    assert!(matches!(err, Error::Message(_)), "{err}");
}

#[test]
fn load_skill_routed_to_inner_module() {
    // A real workspace: election resolves the branch's followed config
    // commit before it reaches the install pool (ARCH §3.3).
    let (_h, repo) = crate::workspace::fixture::workspace();
    crate::workspace::fixture::spawn_root(&repo, "a1");
    let home = tempfile::TempDir::new().unwrap();
    let skill = home.path().join("skills/git-ops");
    std::fs::create_dir_all(&skill).unwrap();
    std::fs::write(skill.join("SKILL.md"), b"body").unwrap();

    // The pool resolves from the `LITANY_HOME`-collapsed data root (§3.3).
    let mut env = stub_env(&repo, "a1");
    env.0
        .insert("LITANY_HOME", home.path().as_os_str().to_owned());

    let input = serde_json::json!({"name":"git-ops"}).to_string();
    let mut stdin = Cursor::new(input.into_bytes());
    let (mut stdout, mut stderr) = (Vec::new(), Vec::new());
    let code = run_with(
        "load_skill",
        &mut stdin,
        &mut stdout,
        &mut stderr,
        &env,
        &StubSpawner,
        &StubSender,
    )
    .unwrap();
    assert_eq!(code, 0);
    let payload: serde_json::Value = serde_json::from_slice(&stdout).unwrap();
    assert_eq!(payload["status"], "loaded");
}

#[test]
fn load_skill_error_is_carried_through_dispatcher() {
    // Repo+branch but no workspace behind them — surfaces as
    // load_skill::Error::Lineage via #[from] into Error::LoadSkill.
    let repo = tempfile::TempDir::new().unwrap();
    let env = stub_env(repo.path(), "a1");
    let input = serde_json::json!({"name":"git-ops"}).to_string();
    let mut stdin = Cursor::new(input.into_bytes());
    let (mut stdout, mut stderr) = (Vec::new(), Vec::new());
    let err = run_with(
        "load_skill",
        &mut stdin,
        &mut stdout,
        &mut stderr,
        &env,
        &StubSpawner,
        &StubSender,
    )
    .unwrap_err();
    assert!(matches!(err, Error::LoadSkill(_)), "{err}");
}

#[test]
fn dispatch_error_is_carried_through_dispatcher() {
    // No env vars set on the StubEnv variant below — surfaces as
    // dispatch::Error::MissingEnv via #[from] into Error::Dispatch.
    struct EmptyEnv;
    impl dispatch::EnvLookup for EmptyEnv {
        fn get(&self, _key: &str) -> Option<std::ffi::OsString> {
            None
        }
    }
    let input = serde_json::json!({"role":"worker","goal":"g"}).to_string();
    let mut stdin = Cursor::new(input.into_bytes());
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();
    let err = run_with(
        "dispatch",
        &mut stdin,
        &mut stdout,
        &mut stderr,
        &EmptyEnv,
        &StubSpawner,
        &StubSender,
    )
    .unwrap_err();
    assert!(matches!(err, Error::Dispatch(_)), "{err}");
}
