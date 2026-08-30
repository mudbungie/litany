//! `.githooks/reference-transaction`: every advance of local `main` reaches
//! `origin`, and nothing else does.
//!
//! The hook under test is the shipped file itself, copied out of
//! `.githooks/` into each fixture's own hooks directory. Only that one hook
//! is copied — pointing `core.hooksPath` at the real `.githooks/` would also
//! arm `pre-commit`, which rejects commits on `main` and runs `make check`.
//!
//! Every fixture gets a **local bare repository** as its `origin`; no test
//! here can reach the real remote.

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use tempfile::TempDir;

/// A `git` invocation scrubbed of the ambient repository: a run under the
/// pre-commit gate inherits `GIT_DIR`/`GIT_INDEX_FILE` from the hook that
/// spawned it, which would aim these commands at the litany repo itself.
fn git(dir: &Path) -> Command {
    let mut cmd = Command::new("git");
    cmd.current_dir(dir)
        .env_remove("GIT_DIR")
        .env_remove("GIT_WORK_TREE")
        .env_remove("GIT_INDEX_FILE")
        .env_remove("GIT_OBJECT_DIRECTORY")
        .env("GIT_AUTHOR_NAME", "litany-test")
        .env("GIT_AUTHOR_EMAIL", "test@example.invalid")
        .env("GIT_COMMITTER_NAME", "litany-test")
        .env("GIT_COMMITTER_EMAIL", "test@example.invalid");
    // No `core.hooksPath` override here, deliberately: this suite's
    // whole subject is a hook, and the fixture arms it by pointing
    // the test repo's own `core.hooksPath` at a copy — which already
    // shadows whatever the machine's global config points at.
    cmd
}

fn run(cmd: &mut Command) -> Output {
    let out = cmd.output().expect("spawn git");
    assert!(
        out.status.success(),
        "git failed ({}):\n{}",
        out.status,
        String::from_utf8_lossy(&out.stderr),
    );
    out
}

fn stdout(out: &Output) -> String {
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

struct Fixture {
    _tmp: TempDir,
    repo: PathBuf,
    remote: PathBuf,
}

impl Fixture {
    /// A repo with the hook armed, a bare `origin`, and one commit already
    /// on `main` in both.
    fn new() -> Self {
        Self::build(true)
    }

    /// The same, minus the `origin` remote.
    fn without_origin() -> Self {
        Self::build(false)
    }

    fn build(with_origin: bool) -> Self {
        let tmp = TempDir::new().unwrap();
        let repo = tmp.path().join("repo");
        let remote = tmp.path().join("origin.git");
        let hooks = tmp.path().join("hooks");

        fs::create_dir_all(&repo).unwrap();
        fs::create_dir_all(&hooks).unwrap();
        let hook = hooks.join("reference-transaction");
        let source = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join(".githooks")
            .join("reference-transaction");
        fs::copy(&source, &hook).unwrap();
        fs::set_permissions(&hook, fs::Permissions::from_mode(0o755)).unwrap();

        run(git(tmp.path())
            .args(["init", "--bare", "-q", "-b", "main"])
            .arg(&remote));
        run(git(&repo).args(["init", "-q", "-b", "main"]));
        run(git(&repo).args(["config", "core.hooksPath"]).arg(&hooks));
        if with_origin {
            run(git(&repo).args(["remote", "add", "origin"]).arg(&remote));
        }

        let fixture = Self {
            _tmp: tmp,
            repo,
            remote,
        };
        fixture.write("seed.txt", "seed");
        fixture.commit("seed");
        fixture
    }

    fn write(&self, name: &str, body: &str) {
        fs::write(self.repo.join(name), body).unwrap();
    }

    /// Stage everything and commit. Returns the raw output so a test can
    /// assert on the hook's stderr and on the commit having succeeded.
    fn commit(&self, message: &str) -> Output {
        run(git(&self.repo).args(["add", "-A"]));
        let out = git(&self.repo)
            .args(["commit", "-q", "-m", message])
            .output()
            .expect("spawn git commit");
        assert!(
            out.status.success(),
            "the hook blocked a commit ({}):\n{}",
            out.status,
            String::from_utf8_lossy(&out.stderr),
        );
        out
    }

    fn rev(&self, what: &str) -> String {
        stdout(&run(git(&self.repo).args(["rev-parse", what])))
    }

    /// `refs/heads/main` in the bare origin, or `None` when it has none.
    fn origin_main(&self) -> Option<String> {
        let out = git(&self.repo)
            .arg("--git-dir")
            .arg(&self.remote)
            .args(["rev-parse", "--verify", "-q", "refs/heads/main"])
            .output()
            .expect("spawn git rev-parse");
        out.status.success().then(|| stdout(&out))
    }

    fn origin_has_branch(&self, name: &str) -> bool {
        git(&self.repo)
            .arg("--git-dir")
            .arg(&self.remote)
            .args(["rev-parse", "--verify", "-q"])
            .arg(format!("refs/heads/{name}"))
            .output()
            .expect("spawn git rev-parse")
            .status
            .success()
    }
}

fn stderr(out: &Output) -> String {
    String::from_utf8_lossy(&out.stderr).to_string()
}

const WARNING: &str = "auto-push of main to origin failed";

#[test]
fn a_commit_on_main_lands_on_origin() {
    let f = Fixture::new();
    // The seed commit alone should already have pushed.
    assert_eq!(f.origin_main().as_deref(), Some(f.rev("HEAD").as_str()));

    f.write("next.txt", "next");
    f.commit("second");
    assert_eq!(f.origin_main().as_deref(), Some(f.rev("HEAD").as_str()));
}

/// The path that actually matters: `bl close` never runs `git commit`. It
/// squashes with `commit-tree` and advances `main` with `update-ref`, which
/// fires no commit hook and no merge hook — only this one.
#[test]
fn a_plumbing_delivery_onto_main_lands_on_origin() {
    let f = Fixture::new();
    let base = f.rev("HEAD");

    run(git(&f.repo).args(["checkout", "-q", "-b", "work/bl-test"]));
    f.write("delivered.txt", "delivered");
    f.commit("work in progress");
    let tree = f.rev("work/bl-test^{tree}");
    run(git(&f.repo).args(["checkout", "-q", "main"]));

    let squash = stdout(&run(git(&f.repo).args([
        "commit-tree",
        &tree,
        "-p",
        &base,
        "-m",
        "delivered [bl-test]",
    ])));
    run(git(&f.repo).args(["update-ref", "refs/heads/main", &squash, &base]));

    assert_eq!(f.origin_main().as_deref(), Some(squash.as_str()));
}

#[test]
fn a_no_ff_merge_onto_main_lands_on_origin() {
    let f = Fixture::new();
    run(git(&f.repo).args(["checkout", "-q", "-b", "feature"]));
    f.write("feature.txt", "feature");
    f.commit("feature work");
    run(git(&f.repo).args(["checkout", "-q", "main"]));
    run(git(&f.repo).args(["merge", "--no-ff", "--no-edit", "-q", "feature"]));

    assert_eq!(f.origin_main().as_deref(), Some(f.rev("HEAD").as_str()));
}

#[test]
fn work_on_a_side_branch_pushes_nothing() {
    let f = Fixture::new();
    let seeded = f.origin_main().expect("seed reached origin");

    run(git(&f.repo).args(["checkout", "-q", "-b", "work/bl-test"]));
    f.write("side.txt", "side");
    let out = f.commit("side commit");

    assert_eq!(f.origin_main(), Some(seeded), "origin/main must not move");
    assert!(
        !f.origin_has_branch("work/bl-test"),
        "no side branch pushed"
    );
    assert!(!stderr(&out).contains(WARNING), "inert, not failing");
}

#[test]
fn an_unreachable_origin_warns_without_blocking_the_commit() {
    let f = Fixture::new();
    let gone = f.repo.join("no-such-remote.git");
    run(git(&f.repo)
        .args(["remote", "set-url", "origin"])
        .arg(&gone));

    f.write("next.txt", "next");
    // `commit` itself asserts the commit succeeded.
    let out = f.commit("second");

    assert!(
        stderr(&out).contains(WARNING),
        "expected a warning on stderr, got: {}",
        stderr(&out),
    );
}

#[test]
fn a_repo_without_an_origin_is_silent() {
    let f = Fixture::without_origin();
    f.write("next.txt", "next");
    let out = f.commit("second");

    assert!(f.origin_main().is_none(), "nothing was pushed");
    assert!(
        !stderr(&out).contains(WARNING),
        "no origin is not a failure, got: {}",
        stderr(&out),
    );
}
