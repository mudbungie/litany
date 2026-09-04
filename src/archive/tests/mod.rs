//! Unit tests for [`super`] — `bundle` / `replay` (ARCH §9.2).
//!
//! The git ops are exercised through a recording [`StubGit`]: the pure
//! logic (ref enumeration, primary-id derivation, slice copying, every
//! error arm) is covered here without a real repo. Real-git correctness
//! is locked in end-to-end by `tests/bundle_replay_cli.rs`. The cases
//! split by verb into [`bundle`] and [`replay`]; the shared stub and
//! fixtures live here.

use super::*;
use std::cell::{Cell, RefCell};
use std::io;

mod bundle;
mod delete;
mod delete_proposal;
mod replay;

/// A `GitRunner` that records `run` invocations and replays a canned
/// `run_capture` output, with injectable failures on either channel.
/// The ancestry probes the governing lineage makes (`for-each-ref` over
/// `refs/heads/config/`, then `merge-base` per head) answer from their
/// own canned fields, so a test states branch enumeration and config
/// lineage separately.
pub(super) struct StubGit {
    /// Canned stdout returned by every un-specialized `run_capture`.
    capture_out: String,
    /// Canned `for-each-ref refs/heads/config/` output.
    config_refs: String,
    /// When true, the `merge-base` probe fails — the no-shared-ancestry
    /// case, which contributes no lineage ref.
    no_lineage: bool,
    /// When true, the `for-each-ref` lineage enumeration itself fails.
    fail_lineage: bool,
    /// When true, `run_capture` fails.
    fail_capture: bool,
    /// Zero-based `run` index to fail at (`None` = never).
    fail_run_at: Option<usize>,
    pub(super) runs: RefCell<Vec<Vec<String>>>,
    run_idx: Cell<usize>,
}

impl StubGit {
    pub(super) fn new(capture_out: &str) -> Self {
        Self {
            capture_out: capture_out.to_owned(),
            config_refs: CONFIG_REFS.to_owned(),
            no_lineage: false,
            fail_lineage: false,
            fail_capture: false,
            fail_run_at: None,
            runs: RefCell::new(Vec::new()),
            run_idx: Cell::new(0),
        }
    }
    pub(super) fn no_lineage(mut self) -> Self {
        self.no_lineage = true;
        self
    }
    pub(super) fn fail_lineage(mut self) -> Self {
        self.fail_lineage = true;
        self
    }
    pub(super) fn fail_capture(mut self) -> Self {
        self.fail_capture = true;
        self
    }
    pub(super) fn fail_run_at(mut self, idx: usize) -> Self {
        self.fail_run_at = Some(idx);
        self
    }
}

impl GitRunner for StubGit {
    fn run(&self, _dest: &Path, args: &[&str]) -> io::Result<()> {
        let idx = self.run_idx.get();
        self.run_idx.set(idx + 1);
        self.runs
            .borrow_mut()
            .push(args.iter().map(|s| (*s).to_owned()).collect());
        if self.fail_run_at == Some(idx) {
            Err(io::Error::other("stub run fail"))
        } else {
            Ok(())
        }
    }
    fn run_capture(&self, _dest: &Path, args: &[&str]) -> io::Result<String> {
        if self.fail_capture {
            return Err(io::Error::other("stub capture fail"));
        }
        match args.first().copied() {
            Some("for-each-ref") if self.fail_lineage => Err(io::Error::other("stub lineage fail")),
            Some("for-each-ref") => Ok(self.config_refs.clone()),
            Some("merge-base") if self.no_lineage => Err(io::Error::other("no merge base")),
            Some("merge-base") => Ok("basesha".to_owned()),
            _ => Ok(self.capture_out.clone()),
        }
    }
}

pub(super) fn tmp() -> tempfile::TempDir {
    tempfile::TempDir::new().unwrap()
}

/// A tempdir shaped as a current-layout workspace (a `repo.git`
/// directory), so [`super::bundle`]'s §10 layout guard admits it. The
/// `StubGit` fakes the git ops, so `repo.git` need only exist.
pub(super) fn ws_tmp() -> tempfile::TempDir {
    let ws = tmp();
    fs::create_dir_all(ws.path().join("repo.git")).unwrap();
    ws
}

pub(super) fn write(path: &Path, body: &str) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, body).unwrap();
}

pub(super) const REFS: &str = "agents/20260101-p1\nagents/20260101-p1-20260102-c1\n";
pub(super) const AGENT: &str = "20260101-p1";
/// The workspace's config branches, as `for-each-ref` prints them —
/// the governing lineage the bundle must carry beside the subtree
/// (§9.2).
pub(super) const CONFIG_REFS: &str = "refs/heads/config/default\nrefs/heads/config/strict\n";

#[test]
fn error_messages_render() {
    // Exercises every arm's Display so the derived formatting is covered.
    let cases: Vec<ArchiveError> = vec![
        ArchiveError::Layout(workspace::LayoutError::NotAWorkspace(PathBuf::from("/x"))),
        ArchiveError::Io(io::Error::other("x")),
        ArchiveError::Git {
            op: "init",
            source: io::Error::other("x"),
        },
        ArchiveError::UnknownAgent("a".into()),
        ArchiveError::BundleMissing(PathBuf::from("/b")),
        ArchiveError::EmptyBundle,
        ArchiveError::MalformedBundle(vec!["a".into()]),
        ArchiveError::DestExists(PathBuf::from("/d")),
    ];
    for e in cases {
        assert!(!format!("{e}").is_empty());
    }
}
