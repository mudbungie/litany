//! §2.9 step 3, the model-call window: the *flag*, not the error's
//! shape, classifies. A stop kills `bz` wherever the stream happens to
//! be, so the harness error it leaves behind varies — a clean line
//! boundary gives `AdapterHalfStream` (covered in the sibling module),
//! a torn one gives `AdapterJson` — and with a stop pending each is
//! that stop, settling the branch as stopped rather than propagating.
//!
//! The retry loop is bounded by the same flag (§2.10): a stop landing
//! in a backoff pause ends the loop instead of spending a further model
//! call on a `bz` the group SIGTERM can no longer reach.

use super::super::fixtures::*;
use super::deposited_result;
use crate::prompt::adapter::AdapterRunner;
use crate::prompt::step::step_dir_rel;
use crate::prompt::{Sleeper, run};
use brazen::ErrorKind;
use std::cell::RefCell;
use std::ffi::OsString;
use std::io;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

/// A [`Sleeper`] that raises the stop flag while the backoff sleeps —
/// SIGTERM landing inside the §2.10 retry pause.
struct StopWhileSleeping<'a> {
    flag: &'a AtomicBool,
    slept: RefCell<Vec<Duration>>,
}

impl Sleeper for StopWhileSleeping<'_> {
    fn sleep(&self, dur: Duration) {
        self.slept.borrow_mut().push(dur);
        self.flag.store(true, Ordering::SeqCst);
    }
}

/// A retryable provider error, then the stop lands in the backoff. The
/// loop must not re-invoke `bz` — the adapter is scripted with no
/// further reply, so a second model call would panic "called more times
/// than scripted". The branch settles as stopped, not as an adapter
/// failure.
#[test]
fn stop_in_the_retry_backoff_deposits_stopped_without_a_further_model_call() {
    let repo = scaffold_repo(VALID_PER_REPO_PROVIDERS_YAML, Some("body"));
    let harness = scaffold_harness_root();
    let adapter = StubAdapter::scripted([
        StubAdapter::reply_ok(&version_line()),
        StubAdapter::reply_ok(&error_stream(ErrorKind::Transport, "boom")),
    ]);
    let git = StubGit::ok();
    let (clock, id) = (FixedClock::default(), FixedIdGen);
    let (unused, tools) = (StubSleeper::default(), StubToolExecutor::ok());
    let stop = AtomicBool::new(false);
    let sleeper = StopWhileSleeping {
        flag: &stop,
        slept: RefCell::new(Vec::new()),
    };
    let mut deps = valid_deps(&adapter, &unused, &git, &clock, &id, &tools, harness.path());
    deps.sleeper = &sleeper;
    deps.stop = &stop;

    let branch = run(
        repo.path(),
        "hi",
        None,
        None,
        None,
        crate::prompt::PinnedDocs::none(),
        None,
        None,
        &deps,
    )
    .unwrap();
    assert_eq!(branch, "ct-1-deadbeef");
    assert_eq!(sleeper.slept.borrow().len(), 1, "one backoff was entered");
    assert_eq!(
        adapter.observed.borrow().len(),
        2,
        "version guard + exactly one model call — the stop ended the retry loop"
    );
    assert!(deposited_result(repo.path()).contains("epitaph: stopped"));
}

/// Adapter whose model call is cut down mid-*line*: a valid
/// `message_start`, then a truncated JSON fragment (the kill landed
/// between two `write`s), then the flag. The fragment surfaces as
/// `AdapterJson`, not `AdapterHalfStream` — and the stop check point
/// must still read it as the stop.
struct TornLineMidCall<'a> {
    flag: &'a AtomicBool,
}

impl AdapterRunner for TornLineMidCall<'_> {
    fn run(
        &self,
        _binary: &OsString,
        args: &[&str],
        _stdin: &[u8],
        on_line: &mut dyn FnMut(&[u8]) -> io::Result<()>,
    ) -> io::Result<Vec<u8>> {
        if args.contains(&"--version") {
            let v = version_line();
            on_line(v.trim_ascii_end())?;
            return Ok(Vec::new());
        }
        on_line(br#"{"type":"message_start","v":1,"role":"assistant"}"#)?;
        on_line(br#"{"type":"content_delta","index":0,"delta":{"text_de"#)?;
        self.flag.store(true, Ordering::SeqCst);
        Ok(Vec::new())
    }
}

#[test]
fn torn_line_with_a_stop_pending_deposits_stopped() {
    let repo = scaffold_repo(VALID_PER_REPO_PROVIDERS_YAML, Some("body"));
    let harness = scaffold_harness_root();
    let stop = AtomicBool::new(false);
    let adapter = TornLineMidCall { flag: &stop };
    let git = StubGit::ok();
    let (clock, id) = (FixedClock::default(), FixedIdGen);
    let (sleeper, tools) = (StubSleeper::default(), StubToolExecutor::ok());
    let unused = unreachable_adapter();
    let mut deps = valid_deps(&unused, &sleeper, &git, &clock, &id, &tools, harness.path());
    deps.adapter = &adapter;
    deps.stop = &stop;

    let branch = run(
        repo.path(),
        "hi",
        None,
        None,
        None,
        crate::prompt::PinnedDocs::none(),
        None,
        None,
        &deps,
    )
    .unwrap();
    assert_eq!(branch, "ct-1-deadbeef");
    assert!(deposited_result(repo.path()).contains("epitaph: stopped"));

    // The §2.9 on-disk signature is intact: the torn line was appended
    // verbatim and no terminal `end` closes the file.
    let response = repo
        .path()
        .join(step_dir_rel("ct-1-deadbeef", 1))
        .join("response.json");
    let bytes = std::fs::read(&response).unwrap();
    assert!(
        bytes.windows(8).any(|w| w == b"\"text_de"),
        "the torn line was appended verbatim: {}",
        String::from_utf8_lossy(&bytes)
    );
    assert!(
        !bytes.windows(14).any(|w| w == br#"{"type":"end"}"#),
        "a stopped step's response.json carries no terminal `end`"
    );
}
