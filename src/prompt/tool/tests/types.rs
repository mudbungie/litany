//! Round-trips and small invariants for the on-disk record types and
//! shared helpers in `super::super`.

use super::super::{
    DEFAULT_TOOL_DEADLINE, EXTERNAL_PREFIX, IN_PROCESS_SUBCOMMAND, INPUT_FILE, OUTPUT_FILE,
    STEP_TOOLS_SUBDIR, TOOLS_DIR, ToolInputRecord, ToolOutputRecord, atomic_write_json,
    tool_call_dir,
};
use crate::prompt::Clock;
use serde_json::json;
use std::path::Path;
use std::time::Duration;
use tempfile::TempDir;

#[test]
fn fixed_clock_emits_distinct_iso_and_compact_strings() {
    // Covers `FixedClock::now_compact` — the trait demands it but the
    // executor never reads it; the existence of this test pins both
    // methods against accidental removal.
    let c = super::fixtures::FixedClock::default();
    assert_eq!(c.now_iso8601(), "iso-1");
    assert_eq!(c.now_iso8601(), "iso-2");
    assert_eq!(c.now_compact(), "ct");
}

#[test]
fn pinned_constants_match_arch_3_3() {
    // Touching these requires touching ARCH §3.3 in the same review.
    assert_eq!(TOOLS_DIR, "tools");
    assert_eq!(EXTERNAL_PREFIX, "litany-tool-");
    assert_eq!(IN_PROCESS_SUBCOMMAND, "tool");
    assert_eq!(STEP_TOOLS_SUBDIR, "tools");
    assert_eq!(INPUT_FILE, "input.json");
    assert_eq!(OUTPUT_FILE, "output.json");
    assert_eq!(DEFAULT_TOOL_DEADLINE, Duration::from_secs(5));
}

#[test]
fn tool_call_dir_lives_under_step_tools() {
    let p = tool_call_dir(Path::new("/x/steps/cid/001"), "toolu_01");
    assert_eq!(p, Path::new("/x/steps/cid/001/tools/toolu_01"));
}

#[test]
fn input_record_round_trips_with_arbitrary_input() {
    let rec = ToolInputRecord {
        id: "toolu_01".into(),
        name: "bash".into(),
        input: json!({"command": "echo hi", "nested": {"k": [1, 2, 3]}}),
    };
    let bytes = serde_json::to_vec(&rec).unwrap();
    let back: ToolInputRecord = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(rec, back);
}

#[test]
fn output_record_keys_are_the_arch_pinned_set() {
    let rec = ToolOutputRecord {
        stdout: "out".into(),
        stderr: "err".into(),
        exit_code: 0,
        started_at: "iso-1".into(),
        ended_at: "iso-2".into(),
    };
    let v: serde_json::Value = serde_json::from_str(&serde_json::to_string(&rec).unwrap()).unwrap();
    for key in ["stdout", "stderr", "exit_code", "started_at", "ended_at"] {
        assert!(v.get(key).is_some(), "missing pinned key: {key}");
    }
}

#[test]
fn atomic_write_lands_final_file_and_removes_temp() {
    let dir = TempDir::new().unwrap();
    let value = json!({"k": "v"});
    atomic_write_json(dir.path(), "x.json", &value).unwrap();
    let final_path = dir.path().join("x.json");
    let temp_path = dir.path().join("x.json.tmp");
    assert!(final_path.is_file(), "final missing");
    assert!(!temp_path.exists(), "temp left behind: {:?}", temp_path);
    let contents: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&final_path).unwrap()).unwrap();
    assert_eq!(contents, value);
}

#[test]
fn atomic_write_surfaces_io_failure_when_dir_missing() {
    let dir = TempDir::new().unwrap();
    let bad = dir.path().join("does-not-exist");
    let err = atomic_write_json(&bad, "x.json", &json!({})).unwrap_err();
    let msg = err.to_string();
    assert!(msg.contains("i/o landing tool record"), "got: {msg}");
}

#[test]
fn atomic_write_surfaces_rename_failure_when_target_is_a_nonempty_directory() {
    // The temp file lands fine; the rename onto an existing non-empty
    // directory fails with ENOTEMPTY/EEXIST and drives the second
    // branch in [`atomic_write_json`].
    let dir = TempDir::new().unwrap();
    let blocker = dir.path().join("x.json");
    std::fs::create_dir(&blocker).unwrap();
    std::fs::write(blocker.join("a-child"), b"hi").unwrap();
    let err = atomic_write_json(dir.path(), "x.json", &json!({"k": 1})).unwrap_err();
    let msg = err.to_string();
    assert!(msg.contains("i/o landing tool record"), "got: {msg}");
}
