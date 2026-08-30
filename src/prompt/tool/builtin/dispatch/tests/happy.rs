//! Happy-path assertions for [`super::super::run`]: input parsing,
//! arg forwarding to the spawner, handle JSON shape, and the
//! production [`super::super::ProcessEnv`] smoke check.

use super::super::*;
use super::fixtures::{StubSpawner, env, fake_repo, input_for};
use std::io::Cursor;

#[test]
fn happy_path_writes_handle_json_and_forwards_args() {
    let (_h, repo) = fake_repo("worker");
    let mut stdin = Cursor::new(input_for("worker", "do the thing"));
    let mut stdout = Vec::new();
    let env = env(&repo, "p1-conv");
    let spawner = StubSpawner::ok("p1-conv-ct9-feedface");

    run(&mut stdin, &mut stdout, &env, &spawner).unwrap();

    let payload: serde_json::Value = serde_json::from_slice(&stdout).unwrap();
    assert_eq!(payload["status"], "in_progress");
    assert_eq!(payload["handle"], "p1-conv-ct9-feedface");

    let invocations = spawner.invocations.borrow();
    assert_eq!(invocations.len(), 1);
    assert_eq!(invocations[0].0, "worker");
    assert_eq!(invocations[0].1, repo);
    assert_eq!(invocations[0].2, "p1-conv");
    assert_eq!(invocations[0].3, "do the thing");
    assert_eq!(
        invocations[0].4, None,
        "no `name` key means an unnamed child"
    );
}

#[test]
fn an_optional_name_input_is_forwarded_to_the_verb() {
    // §2.3: the tool's `name` input is the same fact `litany dispatch
    // --name` carries — the built-in adds no policy of its own, it only
    // forwards, and the verb enforces availability.
    let (_h, repo) = fake_repo("worker");
    let raw = serde_json::json!({ "role": "worker", "goal": "g", "name": "pale-otter" });
    let mut stdin = Cursor::new(raw.to_string().into_bytes());
    let mut stdout = Vec::new();
    let env = env(&repo, "p1");
    let spawner = StubSpawner::ok("p1-sub");

    run(&mut stdin, &mut stdout, &env, &spawner).unwrap();
    let invocations = spawner.invocations.borrow();
    assert_eq!(invocations[0].4.as_deref(), Some("pale-otter"));
}

#[test]
fn handle_is_trimmed_of_trailing_whitespace() {
    // `litany dispatch worker` prints with a trailing newline; the
    // handle on the wire must not carry it.
    let (_h, repo) = fake_repo("worker");
    let mut stdin = Cursor::new(input_for("worker", "g"));
    let mut stdout = Vec::new();
    let env = env(&repo, "p1");
    let spawner = StubSpawner::ok("p1-sub  ");

    run(&mut stdin, &mut stdout, &env, &spawner).unwrap();
    let payload: serde_json::Value = serde_json::from_slice(&stdout).unwrap();
    assert_eq!(payload["handle"], "p1-sub");
}

#[test]
fn process_env_reads_live_var() {
    // Production [`ProcessEnv`] just defers to std::env. Pick a var
    // that is always set on Linux test runs (PATH).
    let p = ProcessEnv;
    assert!(p.get("PATH").is_some());
    assert!(p.get("DEFINITELY_NOT_SET_LITANY_TEST_VAR_xxxxx").is_none());
}

#[test]
fn subprocess_spawner_with_exe_returns_captured_output() {
    // Pin the exe to `true` so the subprocess exits 0 with empty
    // stdio without touching a real litany binary; the wrapper's
    // job is to capture and surface, regardless of what the child
    // produced.
    let s = SubprocessSpawner::with_exe(PathBuf::from("true"));
    // With a name, so the `--name` arm of the argv build runs too.
    let out = s
        .dispatch("worker", Path::new("/tmp"), "p1", "g", Some("pale-otter"))
        .expect("true exits cleanly");
    assert_eq!(out.exit, 0);
    assert!(out.stdout.is_empty());
    assert!(out.stderr.is_empty());
}

#[test]
fn subprocess_spawner_with_exe_returns_nonzero_for_failing_binary() {
    // `false` exits 1 unconditionally; the wrapper preserves the
    // exit code and empty stdio without inventing an io error.
    let s = SubprocessSpawner::with_exe(PathBuf::from("false"));
    let out = s
        .dispatch("worker", Path::new("/tmp"), "p1", "g", None)
        .expect("false runs");
    assert_eq!(out.exit, 1);
}

#[test]
fn subprocess_spawner_with_exe_surfaces_spawn_error_for_missing_binary() {
    // No binary at the given path — Command::output returns io error.
    let s = SubprocessSpawner::with_exe(PathBuf::from("/no/such/litany-binary"));
    let err = s
        .dispatch("worker", Path::new("/tmp"), "p1", "g", None)
        .unwrap_err();
    assert_eq!(err.kind(), io::ErrorKind::NotFound);
}
