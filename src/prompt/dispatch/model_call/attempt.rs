//! One `bz` attempt: the stream read, and the **framing** it settles to
//! (ARCH §4.4). Split from [`super`], which owns the retry loop, so each
//! file holds one question — this one is *what did this attempt say?*,
//! that one is *how many attempts do we make?*.

use super::super::staging::StagingWriter;
use super::{ModelCall, SegmentOutcome, stderr};
use crate::prompt::Error;
use brazen::{CanonicalError, Event, FinishReason};
use std::fs::File;
use std::io::Write;

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
pub(super) fn run_attempt(
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
    let mut truncated = false;
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
                            Event::Finish { reason } => {
                                truncated = *reason == FinishReason::Length;
                            }
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
    if truncated {
        return Ok(SegmentOutcome::Truncated);
    }
    Ok(SegmentOutcome::Complete { handshake_v })
}
