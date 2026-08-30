//! Shared test scaffolding for the dispatch built-in: env-var stub,
//! subprocess-spawn stub, fake-conv-repo helper, and a few small
//! constructors that keep the per-test setup terse.

use super::super::*;
use std::cell::RefCell;
use std::collections::HashMap;
use tempfile::TempDir;

/// Minimal stub [`EnvLookup`] backed by a HashMap so tests can pin
/// `LITANY_CONV_REPO` / `LITANY_CONV_BRANCH` without touching the
/// process env (cargo test runs in parallel; mutating env is racy).
pub(super) struct StubEnv(pub(super) HashMap<&'static str, OsString>);

impl EnvLookup for StubEnv {
    fn get(&self, key: &str) -> Option<OsString> {
        self.0.get(key).cloned()
    }
}

pub(super) fn env(repo: &Path, branch: &str) -> StubEnv {
    let mut m = HashMap::new();
    m.insert(
        crate::prompt::tool::ENV_CONV_REPO,
        repo.as_os_str().to_owned(),
    );
    m.insert(crate::prompt::tool::ENV_CONV_BRANCH, OsString::from(branch));
    StubEnv(m)
}

/// One recorded `litany dispatch` invocation: role, repo, branch, goal,
/// and the optional `--name` (§2.3).
pub(super) type Invocation = (String, PathBuf, String, String, Option<String>);

/// Stub spawner records the call args and returns a canned outcome.
pub(super) struct StubSpawner {
    pub(super) out: DispatchOutput,
    pub(super) invocations: RefCell<Vec<Invocation>>,
}

impl StubSpawner {
    pub(super) fn ok(handle: &str) -> Self {
        Self {
            out: DispatchOutput {
                stdout: format!("{handle}\n"),
                stderr: String::new(),
                exit: 0,
            },
            invocations: RefCell::new(Vec::new()),
        }
    }
    pub(super) fn failing(stderr: &str, exit: i32) -> Self {
        Self {
            out: DispatchOutput {
                stdout: String::new(),
                stderr: stderr.to_string(),
                exit,
            },
            invocations: RefCell::new(Vec::new()),
        }
    }
    pub(super) fn empty_stdout() -> Self {
        Self {
            out: DispatchOutput {
                stdout: String::new(),
                stderr: String::new(),
                exit: 0,
            },
            invocations: RefCell::new(Vec::new()),
        }
    }
}

impl Spawner for StubSpawner {
    fn dispatch(
        &self,
        role: &str,
        repo: &Path,
        branch: &str,
        goal: &str,
        name: Option<&str>,
    ) -> Result<DispatchOutput, io::Error> {
        self.invocations.borrow_mut().push((
            role.to_string(),
            repo.to_path_buf(),
            branch.to_string(),
            goal.to_string(),
            name.map(str::to_owned),
        ));
        Ok(DispatchOutput {
            stdout: self.out.stdout.clone(),
            stderr: self.out.stderr.clone(),
            exit: self.out.exit,
        })
    }
}

/// Spawner whose `dispatch` always fails at the io layer — exercises
/// [`Error::Spawn`].
pub(super) struct ErrSpawner;
impl Spawner for ErrSpawner {
    fn dispatch(
        &self,
        _role: &str,
        _repo: &Path,
        _branch: &str,
        _goal: &str,
        _name: Option<&str>,
    ) -> Result<DispatchOutput, io::Error> {
        Err(io::Error::new(io::ErrorKind::NotFound, "no litany binary"))
    }
}

/// Build a real workspace whose config commit lists `role` in
/// `providers.yaml` with a soul at `souls/<role>.md`, plus the parent
/// agent branches the tests' env values name — validation reads the
/// governing config commit of the calling branch (ARCH §2.2), so the
/// ancestry must really exist. Returns `(holder, workspace_path)`.
pub(super) fn fake_repo(role: &str) -> (TempDir, PathBuf) {
    let (holder, ws) = crate::workspace::fixture::workspace();
    let yaml = format!("roles:\n  {role}:\n    provider: anthropic\n    model: sonnet\n",);
    let soul_rel = format!("souls/{role}.md");
    crate::workspace::fixture::amend_config(
        &ws,
        &[
            ("providers.yaml", yaml.as_str()),
            (&soul_rel, "soul body\n"),
        ],
    );
    for parent in ["p1", "p1-conv"] {
        crate::workspace::fixture::spawn_root(&ws, parent);
    }
    (holder, ws)
}

pub(super) fn input_for(role: &str, goal: &str) -> Vec<u8> {
    serde_json::json!({ "role": role, "goal": goal })
        .to_string()
        .into_bytes()
}
