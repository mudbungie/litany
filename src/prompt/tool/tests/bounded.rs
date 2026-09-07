//! The §3.3 **bounded transcript projection** through the production
//! executor (bl-d5fa): with a `tool_output:` policy in hand, each
//! captured stream is bounded before the result envelope is rendered
//! around it, while the diagnostic `output.json` keeps the full bytes.
//! The head+tail split itself is pinned by `super::super::bound`'s own
//! tests; here the subject is the wiring — streams bounded
//! independently, envelope structure never capped, record complete.

use super::super::{OUTPUT_FILE, STEP_TOOLS_SUBDIR, ToolCall, ToolExecutor, ToolOutputRecord};
use super::fixtures::{FixedClock, HarnessRoot, StepDir, driver_target};
use crate::config::ToolOutputBound;
use crate::prompt::tool::SpawnTool;
use serde_json::json;
use std::sync::atomic::AtomicBool;

fn small_bound() -> Option<ToolOutputBound> {
    Some(ToolOutputBound {
        head_bytes: 8,
        tail_bytes: 8,
    })
}

/// An oversized stdout is projected head+tail with the honest marker —
/// stating bytes, lines, the workspace-relative record path and the
/// continuation address that pages the cut middle back (§3.3 *Paging a
/// cut capture*) — while `output.json` keeps every byte and the
/// envelope header survives.
#[test]
fn oversized_stdout_is_bounded_and_the_record_keeps_it_all() {
    let root = HarnessRoot::new();
    root.install(
        "chatty",
        r#"printf 'AAAAAAA\n%s\nZZZZZZZ\n' "midmidmidmidmid""#,
    );
    let clock = FixedClock::default();
    let step = StepDir::new();
    let exec = SpawnTool::new(root.path(), &clock, driver_target());
    let outcome = exec
        .execute(
            ToolCall {
                id: "toolu_big",
                name: "chatty",
                input: &json!({}),
            },
            &step.path,
            &AtomicBool::new(false),
            small_bound(),
        )
        .unwrap();
    let content = String::from_utf8(outcome.content).unwrap();
    // The envelope header is structure, never capped.
    assert!(content.starts_with("Exit code: 0\n"));
    // Head and tail survive; the middle is gone.
    assert!(content.contains("AAAAAAA\n"));
    assert!(content.ends_with("ZZZZZZZ\n"));
    assert!(!content.contains("midmid"));
    // The marker is honest: byte/line counts and the record's
    // workspace-relative home (byte counts only — no tokenizer here).
    assert!(content.contains(
        "[... stdout truncated: 32 bytes / 3 lines total; showing the first \
         8 and last 8 bytes; full record: steps/convid/001/tools/toolu_big/output.json; \
         read the cut middle with read_tool_output, address \
         steps/convid/001/tools/toolu_big/output.json#stdout@8 ...]"
    ));
    // The diagnostic layer lost nothing.
    let dir = step.path.join(STEP_TOOLS_SUBDIR).join("toolu_big");
    let record: ToolOutputRecord =
        serde_json::from_slice(&std::fs::read(dir.join(OUTPUT_FILE)).unwrap()).unwrap();
    assert_eq!(record.stdout, "AAAAAAA\nmidmidmidmidmid\nZZZZZZZ\n");
}

/// The streams are bounded independently: a small stdout rides
/// untouched next to a bounded stderr, and the `--- stderr ---` marker
/// — envelope structure — is never part of what gets capped.
#[test]
fn streams_are_bounded_independently_around_the_envelope() {
    let root = HarnessRoot::new();
    root.install(
        "warns",
        r#"printf 'ok\n'; printf 'EE-head\n%s\nEE-tail\n' "wall-of-warnings" 1>&2; exit 3"#,
    );
    let clock = FixedClock::default();
    let step = StepDir::new();
    let exec = SpawnTool::new(root.path(), &clock, driver_target());
    let outcome = exec
        .execute(
            ToolCall {
                id: "toolu_w",
                name: "warns",
                input: &json!({}),
            },
            &step.path,
            &AtomicBool::new(false),
            small_bound(),
        )
        .unwrap();
    assert!(outcome.is_error);
    let content = String::from_utf8(outcome.content).unwrap();
    assert!(content.starts_with("Exit code: 3\nok\n--- stderr ---\n"));
    assert!(content.contains("[... stderr truncated:"));
    assert!(!content.contains("wall-of-warnings"));
    assert!(content.ends_with("EE-tail\n"));
}

/// No `tool_output:` policy in the governing workflow — the projection
/// is the capture, byte for byte, marker-free (the pre-bl-d5fa shape).
#[test]
fn without_a_policy_the_projection_is_unbounded() {
    let root = HarnessRoot::new();
    root.install(
        "plain",
        r#"printf '%s\n' "0123456789012345678901234567890123456789""#,
    );
    let clock = FixedClock::default();
    let step = StepDir::new();
    let exec = SpawnTool::new(root.path(), &clock, driver_target());
    let outcome = exec
        .execute(
            ToolCall {
                id: "toolu_p",
                name: "plain",
                input: &json!({}),
            },
            &step.path,
            &AtomicBool::new(false),
            None,
        )
        .unwrap();
    let content = String::from_utf8(outcome.content).unwrap();
    assert_eq!(
        content,
        "Exit code: 0\n0123456789012345678901234567890123456789\n"
    );
}
