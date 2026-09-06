//! `remember`'s declines and its two pure helpers — every path that
//! never reaches a commit, split from [`super`] at the per-file cap.

use super::super::*;
use super::{AGENT, StubEnv, caller, fixture, remember};
use crate::template::RealGit;
use std::collections::HashMap;
use std::ffi::OsString;
use std::io::Cursor;

/// The declines that never reach git.
#[test]
fn malformed_blank_and_env_less_calls_are_declined() {
    let (_h, home, ws) = fixture();
    let mut out: Vec<u8> = Vec::new();
    let env = caller(&home, &ws, AGENT);
    let bad = run_with(
        &mut Cursor::new(b"{".to_vec()),
        &mut out,
        &env,
        &RealGit::new(),
    );
    assert!(matches!(bad.unwrap_err(), Error::InvalidJson(_)));
    assert!(matches!(
        remember(&home, &ws, "   \n ").unwrap_err(),
        Error::Blank
    ));

    let mut empty = StubEnv(HashMap::new());
    let input = serde_json::to_vec(&serde_json::json!({ "fact": "f" })).unwrap();
    let e = run_with(
        &mut Cursor::new(input.clone()),
        &mut out,
        &empty,
        &RealGit::new(),
    )
    .unwrap_err();
    assert!(matches!(e, Error::MissingEnv(ENV_CONV_REPO)));
    empty.0.insert(ENV_CONV_REPO, ws.as_os_str().to_owned());
    let e = run_with(&mut Cursor::new(input), &mut out, &empty, &RealGit::new()).unwrap_err();
    assert!(matches!(e, Error::MissingEnv(ENV_CONV_BRANCH)));
}

/// A stdin that cannot be read, a root that will not resolve, and a
/// branch with no lineage behind it — the three remaining arms.
#[test]
fn unreadable_stdin_rootless_env_and_an_unknown_branch_are_declined() {
    let (_h, home, ws) = fixture();
    struct Broken;
    impl Read for Broken {
        fn read(&mut self, _: &mut [u8]) -> io::Result<usize> {
            Err(io::Error::other("no stdin"))
        }
    }
    let mut out: Vec<u8> = Vec::new();
    let env = caller(&home, &ws, AGENT);
    let e = run_with(&mut Broken, &mut out, &env, &RealGit::new()).unwrap_err();
    assert!(matches!(e, Error::StdinRead(_)));

    let input = serde_json::to_vec(&serde_json::json!({ "fact": "f" })).unwrap();
    let mut rootless = HashMap::new();
    rootless.insert(ENV_CONV_REPO, ws.as_os_str().to_owned());
    rootless.insert(ENV_CONV_BRANCH, OsString::from(AGENT));
    let e = run_with(
        &mut Cursor::new(input.clone()),
        &mut out,
        &StubEnv(rootless),
        &RealGit::new(),
    )
    .unwrap_err();
    assert!(matches!(e, Error::Root(_)), "{e}");

    let env = caller(&home, &ws, "20260101-nobody");
    let e = run_with(&mut Cursor::new(input), &mut out, &env, &RealGit::new()).unwrap_err();
    assert!(matches!(e, Error::Lineage(_)), "{e}");
}

/// A long fact is elided on a character boundary in the subject, and the
/// whole of it still lands in the file.
#[test]
fn a_long_fact_is_elided_in_the_subject_and_kept_in_the_file() {
    let long = "é".repeat(super::SUBJECT_BYTES + 10);
    let msg = commit_message(&long, AGENT);
    let subject = msg.lines().next().unwrap();
    assert!(subject.ends_with('…'), "{subject}");
    assert_eq!(
        subject.chars().filter(|c| *c == 'é').count(),
        super::SUBJECT_BYTES
    );
    assert!(msg.contains(AGENT), "{msg}");
    assert_eq!(
        commit_message("short", AGENT).lines().next().unwrap(),
        "facts: short"
    );
}

/// A stdout that cannot be written is the last arm.
#[test]
fn an_unwritable_stdout_is_surfaced() {
    struct Broken;
    impl Write for Broken {
        fn write(&mut self, _: &[u8]) -> io::Result<usize> {
            Err(io::Error::other("no stdout"))
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }
    let e = emit(&mut Broken, AGENT, &Pass::Landed).unwrap_err();
    assert!(matches!(e, Error::Write(_)));
}
