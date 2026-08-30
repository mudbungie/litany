//! The control wire contract (ARCH §3.3 *Tool control*), against real
//! fixture scripts: verdict parsing, the env/cwd handoff, and every
//! fail-closed protocol arm.

use super::{ControlError, ControlRequest, Verdict, consult};
use serde_json::json;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use tempfile::TempDir;

/// Lay down an executable fixture control under `dir`.
fn script(dir: &Path, body: &str) -> PathBuf {
    let path = dir.join("control.sh");
    std::fs::write(&path, format!("#!/bin/sh\n{body}\n")).unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
    path
}

fn request<'a>(input: &'a serde_json::Value) -> ControlRequest<'a> {
    ControlRequest {
        id: "toolu_1",
        name: "bash",
        input,
        role: "worker",
        agent_id: "ct-1-deadbeef",
    }
}

fn consult_fixture(body: &str) -> Result<Verdict, ControlError> {
    let ws = TempDir::new().unwrap();
    let control = script(ws.path(), body);
    let input = json!({"command": "true"});
    consult(
        &control.to_string_lossy(),
        &request(&input),
        ws.path(),
        &AtomicBool::new(false),
    )
}

#[test]
fn a_pass_verdict_parses() {
    let v = consult_fixture("echo '{\"verdict\":\"pass\"}'").unwrap();
    assert_eq!(v, Verdict::Pass);
}

#[test]
fn refuse_and_hold_carry_their_reasons() {
    let v = consult_fixture("echo '{\"verdict\":\"refuse\",\"reason\":\"no\"}'").unwrap();
    assert_eq!(
        v,
        Verdict::Refuse {
            reason: "no".into()
        }
    );
    let v = consult_fixture("echo '{\"verdict\":\"hold\",\"reason\":\"review\"}'").unwrap();
    assert_eq!(
        v,
        Verdict::Hold {
            reason: "review".into()
        }
    );
}

#[test]
fn the_control_reads_the_invocation_and_caller_from_stdin_and_env() {
    // The fixture dumps its stdin and environment into its cwd and
    // passes — proving the whole handoff: the request JSON on stdin,
    // the `LITANY_CONV_*` pair, and cwd = the workspace root (never the
    // agent's own directory).
    let ws = TempDir::new().unwrap();
    let control = script(
        ws.path(),
        "cat > seen-stdin.json\n\
         printf '%s %s' \"$LITANY_CONV_BRANCH\" \"$LITANY_CONV_REPO\" > seen-env\n\
         echo '{\"verdict\":\"pass\"}'",
    );
    let input = json!({"command": "rm -rf /"});
    let v = consult(
        &control.to_string_lossy(),
        &request(&input),
        ws.path(),
        &AtomicBool::new(false),
    )
    .unwrap();
    assert_eq!(v, Verdict::Pass);
    // Dumped into the cwd, which is the workspace root.
    let stdin: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(ws.path().join("seen-stdin.json")).unwrap())
            .unwrap();
    assert_eq!(stdin["id"], "toolu_1");
    assert_eq!(stdin["name"], "bash");
    assert_eq!(stdin["input"]["command"], "rm -rf /");
    assert_eq!(stdin["role"], "worker");
    assert_eq!(stdin["agent_id"], "ct-1-deadbeef");
    let env = std::fs::read_to_string(ws.path().join("seen-env")).unwrap();
    assert!(env.contains("ct-1-deadbeef"), "{env}");
    assert!(env.contains(&*ws.path().to_string_lossy()), "{env}");
}

#[test]
fn a_missing_binary_is_a_spawn_error() {
    let ws = TempDir::new().unwrap();
    let input = json!({});
    let err = consult(
        "/does/not/exist",
        &request(&input),
        ws.path(),
        &AtomicBool::new(false),
    )
    .unwrap_err();
    assert!(matches!(err, ControlError::Spawn { .. }), "{err:?}");
}

#[test]
fn a_nonzero_exit_is_a_protocol_fault_naming_the_stderr() {
    let err = consult_fixture("echo 'guardian down' >&2; exit 3").unwrap_err();
    let ControlError::Protocol { detail, .. } = err else {
        panic!("expected protocol fault, got {err:?}");
    };
    assert!(detail.contains("exited 3"), "{detail}");
    assert!(detail.contains("guardian down"), "{detail}");
}

#[test]
fn unparseable_or_unknown_verdicts_are_protocol_faults() {
    // Not JSON at all.
    let err = consult_fixture("echo not-a-verdict").unwrap_err();
    assert!(matches!(err, ControlError::Protocol { .. }), "{err:?}");
    // JSON, but not in the verdict vocabulary.
    let err = consult_fixture("echo '{\"verdict\":\"maybe\"}'").unwrap_err();
    assert!(matches!(err, ControlError::Protocol { .. }), "{err:?}");
    // A verdict smuggling an unknown field: declined, never ignored — a
    // control that thinks it can rewrite the input learns otherwise.
    let err = consult_fixture("echo '{\"verdict\":\"pass\",\"rewrite\":{\"command\":\"ls\"}}'")
        .unwrap_err();
    assert!(matches!(err, ControlError::Protocol { .. }), "{err:?}");
    // A pass smuggling a reason, and a refuse without one: both faults.
    let err = consult_fixture("echo '{\"verdict\":\"pass\",\"reason\":\"why\"}'").unwrap_err();
    assert!(matches!(err, ControlError::Protocol { .. }), "{err:?}");
    let err = consult_fixture("echo '{\"verdict\":\"refuse\"}'").unwrap_err();
    assert!(matches!(err, ControlError::Protocol { .. }), "{err:?}");
}

#[test]
fn spawn_fault_folds_every_capture_failure_closed() {
    // The spawn arm carries the source; any other executor error folds
    // into the protocol fault — total, and closed either way.
    let err = super::spawn_fault(
        "ctl",
        crate::prompt::ExecError::Spawn {
            name: "ctl".into(),
            source: std::io::Error::other("no exec"),
        },
    );
    assert!(matches!(err, ControlError::Spawn { .. }), "{err:?}");
    let err = super::spawn_fault(
        "ctl",
        crate::prompt::ExecError::Io {
            dir: "/x".into(),
            source: std::io::Error::other("disk"),
        },
    );
    assert!(matches!(err, ControlError::Protocol { .. }), "{err:?}");
}

#[test]
fn a_stop_cascade_fells_the_control_as_killed_by_signal() {
    // The flag is set before the consult, so the first poll SIGTERMs the
    // sleeping control — the §2.9 cascade the seam classifies as the stop.
    // `exec` so the sleep IS the spawned child: SIGTERM fells it
    // directly and its pipe ends close with it (a grandchild would hold
    // the capture open for its full term).
    let ws = TempDir::new().unwrap();
    let control = script(ws.path(), "exec sleep 60");
    let input = json!({});
    let err = consult(
        &control.to_string_lossy(),
        &request(&input),
        ws.path(),
        &AtomicBool::new(true),
    )
    .unwrap_err();
    let ControlError::KilledBySignal { signal, .. } = err else {
        panic!("expected killed-by-signal, got {err:?}");
    };
    assert_eq!(signal, libc::SIGTERM);
}
