//! §3.3 resolution order: harness-root, then PATH, then the injected
//! driver target. Each branch lands in its own test so a regression
//! points at the offending hop.

use super::super::spawn::lookup::which_in_path_env;
use super::super::spawn::{EnvPath, PathLookup};
use super::super::{SpawnTool, ToolCall, ToolExecutor};
use super::fixtures::{
    FixedClock, HarnessRoot, StepDir, after_header, driver_target, write_script,
};
use serde_json::json;
use std::ffi::OsString;
use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use tempfile::TempDir;

/// Test lookup answering every PATH query with one fixed verdict. Lets
/// the second hop be driven without mutating the process PATH.
pub(super) struct StaticPath(pub(super) Option<PathBuf>);
impl PathLookup for StaticPath {
    fn which_on_path(&self, _prefixed_name: &str) -> Option<PathBuf> {
        self.0.clone()
    }
}

#[test]
fn resolves_external_from_harness_root_first() {
    let root = HarnessRoot::new();
    let installed = root.install("greet", "echo from-harness-root");
    let clock = FixedClock::default();
    let step = StepDir::new();
    let exec = SpawnTool::new(root.path(), &clock, driver_target());
    let outcome = exec
        .execute(
            ToolCall {
                id: "tu_1",
                name: "greet",
                input: &json!({}),
            },
            &step.path,
            &AtomicBool::new(false),
            None,
        )
        .unwrap();
    assert!(installed.is_file(), "installed script vanished");
    assert!(!outcome.is_error);
    assert_eq!(after_header(&outcome.content), b"from-harness-root\n");
}

#[test]
fn falls_through_to_path_when_harness_root_missing() {
    // PATH lookup is exercised through `which_in_path_env` rather than
    // mutating the live PATH (which races with parallel tests under
    // edition 2024's unsafe set_var).
    let pathdir = TempDir::new().unwrap();
    let bin = pathdir.path().join("litany-tool-from-path");
    write_script(&bin, "echo found");
    let hit = which_in_path_env("litany-tool-from-path", Some(pathdir.path().as_os_str()));
    assert_eq!(hit, Some(bin));
}

#[test]
fn path_lookup_skips_dirs_without_the_binary() {
    let a = TempDir::new().unwrap();
    let b = TempDir::new().unwrap();
    let bin = b.path().join("litany-tool-second");
    write_script(&bin, "echo b");
    // Concatenate two dirs into a single PATH so split_paths walks both.
    let combined: OsString = std::env::join_paths([a.path(), b.path()]).expect("joinable paths");
    let hit = which_in_path_env("litany-tool-second", Some(&combined));
    assert_eq!(hit, Some(bin));
}

#[test]
fn path_lookup_returns_none_when_unset() {
    assert_eq!(which_in_path_env("litany-tool-x", None), None);
}

#[test]
fn the_production_lookup_reads_the_live_path() {
    // [`EnvPath`] is what `SpawnTool::new` wires for the second hop; the
    // assertions above drive `which_in_path_env` with a constructed path,
    // so this pins the one production edge — the live-`PATH` read — with
    // a name no install could plausibly carry.
    assert_eq!(
        EnvPath.which_on_path("litany-tool-definitely-not-installed"),
        None
    );
}

#[test]
fn resolves_external_via_path_when_harness_root_misses() {
    // Drop a real script in a tempdir and tell the resolver to return
    // it from PATH lookup. Drives the second hop in `resolve`
    // (line 92→93) without mutating the live PATH env.
    let root = HarnessRoot::new();
    let path_dir = TempDir::new().unwrap();
    let bin = path_dir.path().join("litany-tool-from-path");
    write_script(&bin, "echo hit-via-path");
    let clock = FixedClock::default();
    let step = StepDir::new();
    let exec = SpawnTool::new(root.path(), &clock, driver_target())
        .with_path_lookup(Box::new(StaticPath(Some(bin))));
    let outcome = exec
        .execute(
            ToolCall {
                id: "tu_p",
                name: "from-path",
                input: &json!({}),
            },
            &step.path,
            &AtomicBool::new(false),
            None,
        )
        .unwrap();
    assert!(!outcome.is_error);
    assert_eq!(after_header(&outcome.content), b"hit-via-path\n");
}

#[test]
fn falls_back_to_the_injected_driver_target_when_external_missing() {
    let root = HarnessRoot::new();
    let scripts = TempDir::new().unwrap();
    // Pretend `scripts/fake-litany` is the injected driver target; when
    // invoked with `tool greet …`, echo the args so the test can confirm
    // the third hop's argv shape.
    let fake_litany = scripts.path().join("fake-litany");
    write_script(&fake_litany, r#"echo "$@""#);
    let clock = FixedClock::default();
    let step = StepDir::new();
    let exec = SpawnTool::new(root.path(), &clock, &fake_litany)
        .with_path_lookup(Box::new(StaticPath(None)));
    let outcome = exec
        .execute(
            ToolCall {
                id: "tu_1",
                name: "greet",
                input: &json!({"k": "v"}),
            },
            &step.path,
            &AtomicBool::new(false),
            None,
        )
        .unwrap();
    assert!(!outcome.is_error);
    // The stand-in echoed `tool greet`, confirming the third hop is
    // built per §3.3 ("addressed as `litany tool <name>`") against the
    // *injected* target — not `current_exe`, which under this test
    // binary (and under a linked host) is a different image entirely.
    assert_eq!(after_header(&outcome.content), b"tool greet\n");
}
