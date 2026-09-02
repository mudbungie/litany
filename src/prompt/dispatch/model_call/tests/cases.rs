//! The retry driver's segment, attempt-cap, and handshake contract
//! (ARCH §2.10, §4.4): attempt segments, retry classification, and the
//! stream-framing errors. The §2.9 stop bound on the same loop lives in
//! `stop.rs`, the §2.3 stderr capture in `stderr.rs`; the shared
//! scaffolding in `mod.rs`.

use super::*;

#[test]
fn single_attempt_completes_and_writes_one_segment() {
    let ((r, sleeps, stdins), bytes) = drive(
        vec![Ok(text_stream("hi", FinishReason::Stop))],
        retry(3),
        false,
    );
    r.unwrap();
    assert_eq!(sleeps, 0, "no retry, no sleep");
    assert_eq!(segment::classify(&bytes), Outcome::Complete);
    assert_eq!(stdins[0], b"{}");
}

#[test]
fn retryable_error_then_clean_writes_two_segments() {
    // §12 forced-retry criterion: a retryable 529 then a clean stream.
    let ((r, sleeps, stdins), bytes) = drive(
        vec![
            Ok(error_stream(ErrorKind::Provider { status: 529 })),
            Ok(text_stream("recovered", FinishReason::Stop)),
        ],
        retry(3),
        false,
    );
    r.unwrap();
    assert_eq!(sleeps, 1, "one backoff drove the single retry");
    assert_eq!(ends(&bytes), 2, "two attempt segments");
    assert_eq!(segment::classify(&bytes), Outcome::Complete);
    assert_eq!(stdins[0], stdins[1], "identical re-issued request");
}

/// The delay the loop actually slept before its one retry, given a
/// retryable failure carrying `hint` as its `Retry-After` (§4.4).
fn slept_before_retry(hint: Option<u32>) -> Duration {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("steps/c/001/response.json");
    let sleeper = RecSleeper::default();
    let stop = AtomicBool::new(false);
    let replies = vec![
        Ok(paced_error_stream(
            ErrorKind::Provider { status: 429 },
            hint,
        )),
        Ok(text_stream("recovered", FinishReason::Stop)),
    ];
    let (r, _) = run_injected(
        &path,
        StubAdapter::new(replies),
        retry(3),
        false,
        &sleeper,
        &stop,
    );
    r.unwrap();
    let slept = sleeper.0.borrow();
    assert_eq!(slept.len(), 1, "one backoff drove the single retry");
    slept[0]
}

/// §4.4: the pacing hint is a floor on the config schedule — the three
/// readings, driven through the real retry loop.
#[test]
fn the_pacing_hint_floors_the_config_backoff() {
    // Absent → the pure config schedule (first exponential rung).
    let scheduled = slept_before_retry(None);
    assert_eq!(scheduled, Duration::from_millis(250));
    // Below → the config schedule is unchanged, never shrunk.
    assert_eq!(slept_before_retry(Some(0)), scheduled);
    // Above → the provider's hint wins.
    assert_eq!(slept_before_retry(Some(7)), Duration::from_secs(7));
}

#[test]
fn non_retryable_error_aborts_without_retry() {
    let ((r, sleeps, _), bytes) = drive(
        vec![Ok(error_stream(ErrorKind::Provider { status: 400 }))],
        retry(3),
        false,
    );
    assert!(matches!(r, Err(Error::AdapterError { .. })));
    assert_eq!(sleeps, 0);
    assert_eq!(segment::classify(&bytes), Outcome::Failed);
}

#[test]
fn retryable_error_exhausts_attempt_cap() {
    let ((r, sleeps, _), _) = drive(
        vec![Ok(error_stream(ErrorKind::Transport))],
        retry(1),
        false,
    );
    assert!(matches!(r, Err(Error::AdapterError { .. })));
    assert_eq!(sleeps, 0);
}

#[test]
fn half_stream_is_a_harness_error() {
    let mut bytes = line(&Event::message_start(None, None, Role::Assistant));
    bytes.extend(line(&Event::ContentDelta {
        index: 0,
        delta: Delta::TextDelta("par".into()),
    }));
    let ((r, _, _), _) = drive(vec![Ok(bytes)], retry(3), false);
    assert!(matches!(r, Err(Error::AdapterHalfStream { .. })));
}

#[test]
fn malformed_event_line_surfaces_adapter_json_error() {
    let ((r, _, _), _) = drive(vec![Ok(b"not json\n".to_vec())], retry(3), false);
    assert!(matches!(r, Err(Error::AdapterJson(_))));
}

#[test]
fn a_missing_adapter_surfaces_as_adapter_missing_at_the_model_call_too() {
    // The model-call seam classifies a launch failure exactly as the
    // version guard does: an override or host target that is not there
    // fails here rather than at the guard (which the override skips,
    // §4.4), so the actionable voice must reach both.
    let ((r, _, _), _) = drive(
        vec![Err(io::Error::new(io::ErrorKind::NotFound, "no bz"))],
        retry(3),
        false,
    );
    let Err(err) = r else {
        panic!("expected a spawn failure")
    };
    assert!(matches!(err, Error::AdapterMissing { .. }), "{err}");
    assert!(
        err.to_string()
            .starts_with("provider adapter \"bz\" not found"),
        "{err}"
    );
}

#[test]
fn adapter_override_handshake_accepts_v1() {
    let ((r, _, _), _) = drive(
        vec![Ok(text_stream("ok", FinishReason::Stop))],
        retry(3),
        true,
    );
    assert!(r.is_ok());
}

#[test]
fn adapter_override_handshake_rejects_wrong_version() {
    let mut bytes = line(&Event::MessageStart {
        v: 2,
        id: None,
        model: None,
        role: Role::Assistant,
    });
    bytes.extend(line(&Event::Finish {
        reason: FinishReason::Stop,
    }));
    bytes.extend(line(&Event::End));
    let ((r, _, _), _) = drive(vec![Ok(bytes)], retry(3), true);
    assert!(matches!(
        r,
        Err(Error::HandshakeMismatch {
            found: Some(2),
            expected: 1
        })
    ));
}

#[test]
fn response_path_that_is_a_directory_surfaces_io_error() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("resp");
    std::fs::create_dir_all(&path).unwrap(); // occupy the path with a dir
    let (r, _, _) = run_at(&path, vec![], retry(3), false);
    assert!(matches!(r, Err(Error::Io(_))));
}

#[test]
fn parent_creation_failure_surfaces_io_error() {
    // A regular file where a step dir is expected → create_dir_all fails.
    let dir = tempfile::tempdir().unwrap();
    let blocker = dir.path().join("blocker");
    std::fs::write(&blocker, b"x").unwrap();
    let path = blocker.join("001/response.json");
    let (r, _, _) = run_at(
        &path,
        vec![Ok(text_stream("x", FinishReason::Stop))],
        retry(3),
        false,
    );
    assert!(matches!(r, Err(Error::Io(_))));
}

#[test]
fn real_sleeper_sleeps_without_panicking() {
    RealSleeper.sleep(Duration::ZERO);
}
