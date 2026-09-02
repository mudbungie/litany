//! The model call: exec `bz` once per attempt and own the retry loop
//! (ARCH §4.4, §2.10, §3.5). The typed request every attempt sends is
//! composed apart, in [`super::canonical`].
//!
//! brazen never retries — each `bz` process performs exactly one HTTP
//! round-trip (§4.4). The harness owns the retry loop: on an in-band
//! `Error` event whose kind is retryable ([`CanonicalError::retryable`],
//! the linked crate's single home for the fact — never re-derived), it
//! re-invokes `bz` with the *identical* request (context assembly is a
//! pure function of the step's recorded commit tree, §2.3, so no drift)
//! up to the `workflow.yaml` attempt cap, sleeping the backoff between
//! attempts.
//!
//! **Fd held open for the whole model call (§3.5).** The `response.json`
//! fd is opened once at the first attempt and held across *every*
//! attempt and *every* backoff sleep — closed only at step resolution.
//! fd-open is the single `in_flight` signal, so a mid-retry `Error`
//! segment never reads as `failed` while the loop is still pending.
//! Each attempt's stdout is appended verbatim as one segment; the last
//! segment is authoritative (§4.4).
//!
//! **The stop flag bounds the loop, and classifies its outcome (§2.9).**
//! A pending stop ends the loop instead of launching another `bz`: a
//! process spawned after the group SIGTERM is outside the cascade's
//! reach, so retrying through a stop would spend a whole further model
//! call — the window that dominates a stop's observed latency. And the
//! error this module hands back under a pending stop means nothing on
//! its own: a kill lands wherever the adapter was, so the stop leaves
//! [`Error::AdapterHalfStream`] on a clean line boundary,
//! [`Error::AdapterJson`] on a torn one, or an `AdapterError` from an
//! attempt that had already failed — indistinguishable from the genuine
//! article by shape. The flag is the only reliable witness, so the
//! callers' §2.9 step-3 check points discard *whatever* came back when
//! it is set, and propagate it as a fault when it is not.
//!
//! The adapter's stderr rides beside the call — see [`stderr`].

mod stderr;

use super::staging::{StagingWriter, staging_path_for};
use super::stop_signal;
use crate::config::RetryConfig;
use crate::prompt::Error;
use crate::prompt::adapter::AdapterRunner;
use brazen::{CanonicalError, EVENT_SCHEMA_VERSION, Event};
use std::ffi::OsString;
use std::fs::File;
use std::io::Write;
use std::path::Path;
use std::sync::atomic::AtomicBool;
use std::time::Duration;

/// How one attempt's segment settled — the framing the retry loop acts
/// on (§4.4, §2.10). Content lives only in the staging sink (§2.3), so
/// no assembled blocks ride here.
enum SegmentOutcome {
    /// Trailing `end`, no `error`: the model call completed (§4.4). The
    /// handshake version stamped on the first `message_start`, if any,
    /// rides along for the adapter-override guard (§4.4).
    Complete { handshake_v: Option<u8> },
    /// An in-band `error` event: the retry loop classifies retryability
    /// via [`CanonicalError::retryable`] (§2.10).
    Failed(CanonicalError),
    /// The stream ended without a trailing `end` — killed mid-stream
    /// (§2.9 signature), or an adapter that never reached the contract.
    /// The stderr tail tells a human which; the flag tells the harness.
    HalfStream { stderr_tail: String },
}

/// Injected sleep so the retry backoff is real in production and a
/// no-op in tests (the retry *logic* does not depend on wall time).
pub trait Sleeper {
    fn sleep(&self, dur: Duration);
}

/// Production [`Sleeper`] — blocks the calling thread.
#[derive(Debug, Clone, Copy)]
pub struct RealSleeper;

impl Sleeper for RealSleeper {
    fn sleep(&self, dur: Duration) {
        std::thread::sleep(dur);
    }
}

/// Everything the retry loop needs beyond the request itself.
pub(super) struct ModelCall<'a> {
    pub(super) adapter: &'a dyn AdapterRunner,
    pub(super) sleeper: &'a dyn Sleeper,
    pub(super) binary: &'a OsString,
    pub(super) provider_row: &'a str,
    pub(super) retry: RetryConfig,
    /// The §2.9 stop flag (`Deps::stop`), read at the retry loop's
    /// launch-another-`bz` decision — see [`run`].
    pub(super) stop: &'a AtomicBool,
    /// True under an `adapter:` override (§4.2): the version guard is
    /// skipped and the in-band `MessageStart.v == EVENT_SCHEMA_VERSION`
    /// handshake governs the completed segment instead (§4.4).
    pub(super) expect_handshake: bool,
}

/// Drive one model call to resolution: `bz --json --provider <row>` per
/// attempt, request on stdin, each attempt's stdout appended verbatim to
/// `response_path` as one segment. On success the staging sink is sealed
/// (the model-output transcript entry, §2.3) and `Ok(())` returns — the
/// call's *content and usage* have their one home in that entry, never a
/// return value. A non-retryable / budget-exhausted `Error`, a half-stream
/// kill, or a malformed event surfaces as a harness [`Error`] — which,
/// with a stop pending, the caller reads as the stop (§2.9 step 3).
pub(super) fn run(
    call: &ModelCall<'_>,
    request_bytes: &[u8],
    response_path: &Path,
) -> Result<(), Error> {
    if let Some(parent) = response_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    // One fd, held across every attempt and backoff sleep (§3.5). The
    // staging sink (§2.3) is the second stream off the same pass — the
    // model-output transcript entry under construction (content and usage
    // alike), sealed and renamed by the caller once the call settles.
    let mut response_file = File::create(response_path)?;
    // The adapter's diagnostic channel (§2.3), created unconditionally:
    // 0 bytes is the ordinary record, not an absence to special-case.
    let stderr_path = response_path.with_file_name(crate::prompt::step::STDERR_FILE);
    let mut stderr_file = File::create(&stderr_path)?;
    let mut staging = StagingWriter::create(&staging_path_for(response_path))?;
    let args = ["--json", "--provider", call.provider_row];
    let max = call.retry.max_attempts.max(1);
    let mut attempt = 1;
    loop {
        staging.begin_segment();
        let outcome = run_attempt(
            call,
            &args,
            request_bytes,
            &mut response_file,
            &mut stderr_file,
            &mut staging,
        )?;
        match outcome {
            SegmentOutcome::Complete { handshake_v } => {
                check_handshake(call.expect_handshake, handshake_v)?;
                staging.seal()?;
                drop(response_file);
                return Ok(());
            }
            SegmentOutcome::Failed(err) => {
                // §4.4 segment authority: an `Error`-terminated segment
                // contributes nothing — truncate its blocks from staging.
                staging.truncate_segment()?;
                // §2.10 retry, bounded by the §2.9 stop: the loop must
                // not launch a further `bz` once a stop is pending. That
                // process would be spawned *after* the group SIGTERM, so
                // nothing would fell it and the stop would cost a whole
                // additional model call — the window that dominates wall
                // time. Read on both sides of the backoff: the flag can
                // be set during the attempt (the group signal reaches the
                // executor and `bz` together) or during the sleep itself
                // (`thread::sleep` restarts over EINTR, so the handler's
                // flag is the only evidence it was interrupted). The
                // error returned instead is discarded by the caller's
                // §2.9 step-3 check point, which settles the branch as
                // stopped. The delay is the config schedule floored by
                // the attempt's `Retry-After` pacing hint (§4.4).
                if err.retryable() && attempt < max && !stop_signal::stopped(call.stop) {
                    let d = call.retry.backoff.delay(attempt, err.retry_after_seconds);
                    call.sleeper.sleep(d);
                    if !stop_signal::stopped(call.stop) {
                        attempt += 1;
                        continue;
                    }
                }
                drop(response_file);
                return Err(Error::from_adapter(call.provider_row, err));
            }
            SegmentOutcome::HalfStream { stderr_tail } => {
                // Killed mid-stream (§2.9): nothing settled, so staging
                // is left as debris the step's re-run overwrites (§2.3).
                drop(response_file);
                return Err(Error::AdapterHalfStream {
                    stderr_log: stderr_path,
                    tail: stderr_tail,
                });
            }
        }
    }
}

/// One `bz` attempt: tee every stdout line to the open `response_file`
/// (as a segment) and stream content and usage into the `staging` sink
/// (§2.3), tracking only the segment's *framing* — the terminal `end`, an
/// in-band `error`, and the first `message_start`'s handshake `v`
/// (§4.4). Events after the terminal `end` are ignored (defensive — a
/// buggy adapter emitting stray lines must not corrupt the entry). A
/// malformed event line — or a `content_stop`'d tool-use block whose
/// `json_delta` does not parse — surfaces as [`Error::AdapterJson`]; a
/// tool-use block never `content_stop`'d is caught by the sink's seal
/// (§2.3, [`StagingWriter::seal`]).
fn run_attempt(
    call: &ModelCall<'_>,
    args: &[&str],
    request_bytes: &[u8],
    response_file: &mut File,
    stderr_file: &mut File,
    staging: &mut StagingWriter,
) -> Result<SegmentOutcome, Error> {
    let mut feed_err: Option<serde_json::Error> = None;
    let mut staging_err: Option<Error> = None;
    let mut error: Option<CanonicalError> = None;
    let mut ended = false;
    let mut handshake_v: Option<u8> = None;
    let stderr = call
        .adapter
        .run(call.binary, args, request_bytes, &mut |line| {
            response_file.write_all(line)?;
            response_file.write_all(b"\n")?;
            if feed_err.is_none() && staging_err.is_none() && !ended {
                match serde_json::from_slice::<Event>(line) {
                    Ok(event) => {
                        match &event {
                            Event::MessageStart { v, .. } => handshake_v = Some(*v),
                            Event::Error(e) => error = Some(e.clone()),
                            Event::End => ended = true,
                            _ => {}
                        }
                        // Terminal `end`/`error`/`finish` are no-ops in the
                        // sink (§2.3), `usage` is not — it rides the entry;
                        // stray post-terminal lines the `!ended` guard blocks.
                        if let Err(e) = staging.feed(&event) {
                            staging_err = Some(e);
                        }
                    }
                    Err(e) => feed_err = Some(e),
                }
            }
            Ok(())
        })
        .map_err(|e| crate::prompt::adapter::spawn_error(call.binary, e))?;
    stderr_file.write_all(&stderr)?;
    if let Some(e) = feed_err {
        return Err(Error::AdapterJson(e));
    }
    if let Some(e) = staging_err {
        return Err(e);
    }
    // An `error` segment is `Failed` even if a trailing `end` closed it;
    // no `end` at all is the kill signature (§2.9).
    if let Some(err) = error {
        return Ok(SegmentOutcome::Failed(err));
    }
    if !ended {
        return Ok(SegmentOutcome::HalfStream {
            stderr_tail: stderr::tail(&stderr),
        });
    }
    Ok(SegmentOutcome::Complete { handshake_v })
}

/// Under an `adapter:` override the completed segment must carry a
/// `MessageStart.v` equal to `brazen::EVENT_SCHEMA_VERSION` (§4.4).
fn check_handshake(expect: bool, handshake_v: Option<u8>) -> Result<(), Error> {
    if expect && handshake_v != Some(EVENT_SCHEMA_VERSION) {
        return Err(Error::HandshakeMismatch {
            found: handshake_v,
            expected: EVENT_SCHEMA_VERSION,
        });
    }
    Ok(())
}

#[cfg(test)]
mod tests;
