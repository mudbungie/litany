//! Recording stubs for `prompt::run`'s injected dependencies.
//!
//! Lives alongside [`super::fixtures`] but split out so the latter
//! stays under the repo's per-file line cap.

use crate::prompt::{AdapterRunner, Sleeper, brazen_pin};
use crate::template::GitRunner;
use std::cell::RefCell;
use std::collections::VecDeque;
use std::ffi::OsString;
use std::io;
use std::path::{Path, PathBuf};
use std::time::Duration;

/// Canned [`AdapterRunner`] reply. `Ok` carries raw stdout bytes
/// (with `\n` terminators) that the stub replays into the per-line
/// callback. `Err` short-circuits before any callback fires.
pub(super) enum AdapterReply {
    Ok(Vec<u8>),
    Err(io::Error),
}

/// One adapter invocation: (binary, argv, stdin).
pub(super) type AdapterCall = (OsString, Vec<String>, Vec<u8>);

/// The bytes `bz --version` prints under the pin the harness expects
/// (the load-time version guard, §4.4).
pub(super) fn version_line() -> Vec<u8> {
    format!("bz {}\n", brazen_pin()).into_bytes()
}

/// FIFO-replying [`AdapterRunner`] with a recording log. Each scripted
/// reply's bytes are split on `\n` and replayed through the callback
/// (the §4.4 wire shape: one event per line). Under the default
/// (no `adapter:` override) resolution the harness first runs the
/// version guard (`bz --version`), so a `run`-level script leads with
/// [`version_line`]; adapter-override tests skip that.
pub(super) struct StubAdapter {
    replies: RefCell<VecDeque<AdapterReply>>,
    /// Per-invocation stderr captures (§2.3). An exhausted queue is the
    /// ordinary case: an adapter that says nothing there.
    stderr: RefCell<VecDeque<Vec<u8>>>,
    pub(super) observed: RefCell<Vec<AdapterCall>>,
}

impl StubAdapter {
    pub(super) fn scripted<I>(replies: I) -> Self
    where
        I: IntoIterator<Item = AdapterReply>,
    {
        Self {
            replies: RefCell::new(replies.into_iter().collect()),
            stderr: RefCell::new(VecDeque::new()),
            observed: RefCell::new(Vec::new()),
        }
    }

    /// Version guard reply then one model-call stream (the default
    /// no-override happy path).
    pub(super) fn happy(model_stream: &[u8]) -> Self {
        Self::scripted([
            AdapterReply::Ok(version_line()),
            AdapterReply::Ok(model_stream.to_vec()),
        ])
    }

    /// [`Self::happy`] with the model call's `bz` also writing to
    /// stderr — the startup-failure shape when the stream is empty.
    pub(super) fn happy_with_stderr(model_stream: &[u8], stderr: &[u8]) -> Self {
        let stub = Self::happy(model_stream);
        // The version guard runs first and says nothing on stderr.
        *stub.stderr.borrow_mut() = [Vec::new(), stderr.to_vec()].into();
        stub
    }

    pub(super) fn reply_ok(bytes: &[u8]) -> AdapterReply {
        AdapterReply::Ok(bytes.to_vec())
    }
    pub(super) fn reply_err(kind: io::ErrorKind, msg: &str) -> AdapterReply {
        AdapterReply::Err(io::Error::new(kind, msg.to_string()))
    }
}

impl AdapterRunner for StubAdapter {
    fn run(
        &self,
        binary: &OsString,
        args: &[&str],
        stdin_bytes: &[u8],
        on_line: &mut dyn FnMut(&[u8]) -> io::Result<()>,
    ) -> io::Result<Vec<u8>> {
        self.observed.borrow_mut().push((
            binary.clone(),
            args.iter().map(|s| (*s).to_owned()).collect(),
            stdin_bytes.to_vec(),
        ));
        let bytes = match self.replies.borrow_mut().pop_front() {
            Some(AdapterReply::Ok(b)) => b,
            Some(AdapterReply::Err(e)) => return Err(e),
            None => panic!("StubAdapter::run called more times than scripted"),
        };
        for line in bytes.split(|b| *b == b'\n') {
            if line.is_empty() {
                continue;
            }
            on_line(line)?;
        }
        Ok(self.stderr.borrow_mut().pop_front().unwrap_or_default())
    }
}

pub(super) fn unreachable_adapter() -> StubAdapter {
    StubAdapter::scripted([])
}

/// No-op [`Sleeper`]: the retry loop's backoff sleeps are elided in
/// tests (the retry *logic* does not depend on wall time). Records the
/// requested durations so a test can assert a backoff was scheduled.
#[derive(Default)]
pub(super) struct StubSleeper {
    pub(super) slept: RefCell<Vec<Duration>>,
}

impl Sleeper for StubSleeper {
    fn sleep(&self, dur: Duration) {
        self.slept.borrow_mut().push(dur);
    }
}

/// The fixed sha every capture that answers a revision question
/// returns. 40 hex chars so it is shaped like a real commit id.
pub(super) const STUB_SHA: &str = "cafecafecafecafecafecafecafecafecafecafe";

/// Recording [`GitRunner`] with optional `fail_at` index.
///
/// Captures emulate just enough of git's read surface for the
/// control-from-config-commit reads (ARCH §2.2) to resolve against the
/// plain files [`super::fixtures::scaffold_repo`] lays out: `show
/// <sha>:<path>` reads `<workspace>/<path>` (the stub's stand-in for
/// the config commit's tree; `dest` is `<workspace>/repo.git`),
/// revision questions return [`STUB_SHA`], and everything else
/// captures empty (so e.g. the drain's `status --porcelain` reports a
/// clean tree). The real contract is exercised by the real-git tests
/// (`workspace::tests`, the integration suite).
#[derive(Default)]
pub(super) struct StubGit {
    pub(super) runs: RefCell<Vec<(PathBuf, Vec<String>)>>,
    fail_at: Option<usize>,
}

impl StubGit {
    pub(super) fn ok() -> Self {
        Self::default()
    }
    pub(super) fn failing_at(idx: usize) -> Self {
        Self {
            fail_at: Some(idx),
            ..Self::default()
        }
    }
}

impl GitRunner for StubGit {
    fn run(&self, dest: &Path, args: &[&str]) -> io::Result<()> {
        let mut runs = self.runs.borrow_mut();
        let idx = runs.len();
        runs.push((
            dest.to_path_buf(),
            args.iter().map(|s| (*s).to_owned()).collect(),
        ));
        if self.fail_at == Some(idx) {
            Err(io::Error::other(format!("stub git fail at {idx}")))
        } else {
            Ok(())
        }
    }
    fn run_capture(&self, dest: &Path, args: &[&str]) -> io::Result<String> {
        self.run(dest, args)?;
        match args.first().copied() {
            Some("show") => {
                let spec = args.last().unwrap_or(&"");
                let (_, path) = spec
                    .split_once(':')
                    .ok_or_else(|| io::Error::other("stub show: no <sha>:<path> spec"))?;
                let ws = dest.parent().expect("dest is <workspace>/repo.git");
                std::fs::read_to_string(ws.join(path))
            }
            Some("rev-parse" | "merge-base") => Ok(STUB_SHA.to_string()),
            // The one workspace ref query, in its three formats (§2.3):
            // `%(refname)` for the governing-lineage merge-bases,
            // `%(refname:short)` for the lineage-name pool, and
            // `%(objectname)` for the followed-tip derivation (§2.2,
            // bl-403b) — a tip, so it answers the sha shape.
            Some("for-each-ref") => Ok(if args.contains(&"--format=%(refname:short)") {
                "config/default".to_string()
            } else if args.contains(&"--format=%(objectname)") {
                STUB_SHA.to_string()
            } else {
                "refs/heads/config/default".to_string()
            }),
            _ => Ok(String::new()),
        }
    }
}
