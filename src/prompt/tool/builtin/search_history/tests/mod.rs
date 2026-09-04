//! Unit tests for [`super::run`] over **real** git repositories (ARCH
//! §3.3, `docs/DESIGN_CONTEXT_ECONOMY.md` §4). Every arm of the tool is
//! a git query, so a stubbed runner would only test the stub; these
//! build the workspace shapes production runs against — including a real
//! compaction landing, which is what the §5.4 pin needs.
//!
//! The `{pattern}` listing lives in [`listing`], the declines in
//! [`declines`], and the parser's own contract in [`parse`].

use super::*;
use crate::prompt::compactor::land::{LandOutcome, land};
use std::collections::HashMap;
use std::ffi::OsString;
use std::io::Cursor;
use std::path::PathBuf;
use tempfile::TempDir;

mod declines;
mod listing;
mod parse;

struct StubEnv(HashMap<&'static str, OsString>);
impl EnvLookup for StubEnv {
    fn get(&self, key: &str) -> Option<OsString> {
        self.0.get(key).cloned()
    }
}

fn env(ws: &Path) -> StubEnv {
    let mut m = HashMap::new();
    m.insert(ENV_CONV_REPO, ws.as_os_str().to_owned());
    StubEnv(m)
}

/// A reader that always errors, for the stdin-read arm.
struct FailingReader;
impl Read for FailingReader {
    fn read(&mut self, _buf: &mut [u8]) -> io::Result<usize> {
        Err(io::Error::other("stdin boom"))
    }
}

/// A writer that always errors, for the stdout arm.
struct FailingWriter;
impl Write for FailingWriter {
    fn write(&mut self, _buf: &[u8]) -> io::Result<usize> {
        Err(io::Error::other("stdout boom"))
    }
    fn flush(&mut self) -> io::Result<()> {
        self.write(&[]).map(|_| ())
    }
}

fn g() -> RealGit {
    RealGit::new()
}

/// A workspace whose `repo.git` is a real repository checked out on
/// `agents/p1` — the same object store the bare production one is, so
/// `git log`/`git show` answer identically.
fn workspace_repo() -> (TempDir, PathBuf) {
    let holder = TempDir::new().unwrap();
    let repo = workspace::repo_git(holder.path());
    std::fs::create_dir_all(&repo).unwrap();
    let git = g();
    git.run(&repo, &["init", "-b", "agents/p1"]).unwrap();
    git.run(&repo, &["config", "user.email", "t@t"]).unwrap();
    git.run(&repo, &["config", "user.name", "t"]).unwrap();
    git.run(&repo, &["config", "core.hooksPath", "/dev/null"])
        .unwrap();
    commit(&repo, "step 001: dispatch [p1]", &[("goal.md", "go")], &[]);
    (holder, repo)
}

/// One commit on the current branch: `writes` written, `deletes` removed.
fn commit(repo: &Path, subject: &str, writes: &[(&str, &str)], deletes: &[&str]) {
    for (rel, content) in writes {
        let path = repo.join(rel);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, content).unwrap();
    }
    for rel in deletes {
        g().run(repo, &["rm", "-q", "--", rel]).unwrap();
    }
    g().run(repo, &["add", "-A"]).unwrap();
    g().run(repo, &["commit", "--allow-empty", "-m", subject])
        .unwrap();
}

/// The tool's answer to one input object, as a string.
fn ask(ws: &Path, input: &serde_json::Value) -> String {
    let mut stdin = Cursor::new(input.to_string().into_bytes());
    let mut out = Vec::new();
    run(&mut stdin, &mut out, &env(ws)).unwrap();
    String::from_utf8(out).unwrap()
}

fn decline(ws: &Path, input: &serde_json::Value) -> Error {
    let mut stdin = Cursor::new(input.to_string().into_bytes());
    run(&mut stdin, &mut Vec::new(), &env(ws)).unwrap_err()
}

/// The `<commit>:<path>` addresses a listing names, in order.
fn addresses(answer: &str) -> Vec<&str> {
    answer.lines().take_while(|l| !l.is_empty()).collect()
}

/// The §5.4 pin: the compactor's ref is the soft archive, and it is what
/// the search reaches. A pre-compaction entry is squashed off the
/// dispatching branch by a real landing and is still found — once.
#[test]
fn a_pre_compaction_entry_is_found_through_the_compactors_ref() {
    let (holder, repo) = workspace_repo();
    commit(
        &repo,
        "step 002",
        &[("messages/001-user.md", "needle in the old span\n")],
        &[],
    );
    commit(
        &repo,
        "step 003",
        &[("messages/002-user.md", "needle in the live tail\n")],
        &[],
    );
    let added = g().run_capture(&repo, &["rev-parse", "HEAD~1"]).unwrap();

    // A real compactor: forked at the compaction point, writing a
    // summary and nominating the old entry for deletion.
    g().run(&repo, &["checkout", "-q", "-b", "agents/p1-cmp"])
        .unwrap();
    commit(
        &repo,
        "dispatch: compactor [p1-cmp]",
        &[("goal.md", "compact")],
        &[],
    );
    commit(
        &repo,
        "compaction",
        &[("summary/001.md", "we discussed it\n")],
        &["messages/001-user.md"],
    );
    g().run(&repo, &["checkout", "-q", "agents/p1"]).unwrap();
    assert_eq!(
        land(&repo, "p1", "p1-cmp", None, &g()).unwrap(),
        LandOutcome::Landed
    );

    // The landing squashed the span: the commit that ADDED the entry is
    // gone from the dispatching branch …
    assert!(
        g().run(&repo, &["merge-base", "--is-ancestor", &added, "agents/p1"])
            .is_err()
    );
    // … and stands on the compactor's own ref, the soft archive.
    g().run(
        &repo,
        &["merge-base", "--is-ancestor", &added, "agents/p1-cmp"],
    )
    .unwrap();

    let answer = ask(holder.path(), &serde_json::json!({"pattern": "needle"}));
    let addrs = addresses(&answer);
    let old = format!("{added}:messages/001-user.md");
    assert!(addrs.contains(&old.as_str()), "{answer}");
    assert_eq!(
        ask(holder.path(), &serde_json::json!({"entry": old})),
        "needle in the old span\n"
    );

    // Found ONCE, not per ref: the replay re-added the live tail on
    // `agents/p1` while the original stands on `agents/p1-cmp`, so the
    // naive walk would list the entry twice. One entry, one hit — and
    // the address kept is the live branch's, not the archive's.
    let live: Vec<&&str> = addrs
        .iter()
        .filter(|a| a.ends_with(":messages/002-user.md"))
        .collect();
    assert_eq!(live.len(), 1, "{answer}");
    let sha = live[0].split(':').next().unwrap();
    g().run(&repo, &["merge-base", "--is-ancestor", sha, "agents/p1"])
        .unwrap();
}
