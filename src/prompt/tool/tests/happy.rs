//! End-to-end stdio + on-disk record assertions: the §3.3 *result
//! envelope* the model reads (exit code stated, stdout, then a marked
//! stderr block whenever the child wrote one — success included), and
//! the exact shape of `input.json` / `output.json` per §3.3 "Disk
//! record". The two are deliberately different views of one call: the
//! envelope is what the model saw, the record is what the subprocess
//! emitted.

use super::super::{
    ENV_CONV_BRANCH, ENV_CONV_REPO, INPUT_FILE, OUTPUT_FILE, STEP_TOOLS_SUBDIR, SpawnTool,
    ToolCall, ToolExecutor, ToolInputRecord, ToolOutputRecord,
};
use super::fixtures::{FixedClock, HarnessRoot, StepDir, after_header, driver_target};
use serde_json::json;
use std::sync::atomic::AtomicBool;

#[test]
fn happy_path_writes_input_and_output_records() {
    let root = HarnessRoot::new();
    root.install("greet", r#"printf "hello %s" "$1""#);
    let clock = FixedClock::default();
    let step = StepDir::new();
    let exec = SpawnTool::new(root.path(), &clock, driver_target());
    let outcome = exec
        .execute(
            ToolCall {
                id: "toolu_01",
                name: "greet",
                input: &json!({"who": "world"}),
            },
            &step.path,
            &AtomicBool::new(false),
            None,
        )
        .unwrap();
    assert!(!outcome.is_error);
    assert_eq!(outcome.content, b"Exit code: 0\nhello ");

    let dir = step.path.join(STEP_TOOLS_SUBDIR).join("toolu_01");
    let input: ToolInputRecord =
        serde_json::from_slice(&std::fs::read(dir.join(INPUT_FILE)).unwrap()).unwrap();
    assert_eq!(input.id, "toolu_01");
    assert_eq!(input.name, "greet");
    assert_eq!(input.input, json!({"who": "world"}));

    let output: ToolOutputRecord =
        serde_json::from_slice(&std::fs::read(dir.join(OUTPUT_FILE)).unwrap()).unwrap();
    assert_eq!(output.exit_code, 0);
    assert_eq!(output.stdout, "hello ");
    assert_eq!(output.stderr, "");
    assert_eq!(output.started_at, "iso-1");
    assert_eq!(output.ended_at, "iso-2");
}

#[test]
fn non_zero_exit_states_its_code_and_marks_the_stderr_block() {
    let root = HarnessRoot::new();
    root.install(
        "fail",
        r#"
echo "out-line"
echo "err-line" 1>&2
exit 7
"#,
    );
    let clock = FixedClock::default();
    let step = StepDir::new();
    let exec = SpawnTool::new(root.path(), &clock, driver_target());
    let outcome = exec
        .execute(
            ToolCall {
                id: "toolu_2",
                name: "fail",
                input: &json!({}),
            },
            &step.path,
            &AtomicBool::new(false),
            None,
        )
        .unwrap();
    assert!(outcome.is_error);
    // §3.3: the code is *stated*, not merely flagged — exit 7 is
    // distinguishable from exit 127, which `is_error` alone made one bit.
    assert_eq!(
        outcome.content,
        b"Exit code: 7\nout-line\n--- stderr ---\nerr-line\n"
    );

    let dir = step.path.join(STEP_TOOLS_SUBDIR).join("toolu_2");
    let output: ToolOutputRecord =
        serde_json::from_slice(&std::fs::read(dir.join(OUTPUT_FILE)).unwrap()).unwrap();
    assert_eq!(output.exit_code, 7);
    assert_eq!(output.stdout, "out-line\n");
    assert_eq!(output.stderr, "err-line\n");
}

#[test]
fn exit_zero_still_surfaces_stderr() {
    // The bl-ffc5 defect: stderr on a zero exit used to be dropped from
    // the wire content, so a warning on an otherwise successful command
    // was invisible to the agent unless it thought to redirect `2>&1`.
    let root = HarnessRoot::new();
    root.install(
        "noisy",
        r#"
echo "loud diagnostic" 1>&2
echo "quiet result"
"#,
    );
    let clock = FixedClock::default();
    let step = StepDir::new();
    let exec = SpawnTool::new(root.path(), &clock, driver_target());
    let outcome = exec
        .execute(
            ToolCall {
                id: "tu_n",
                name: "noisy",
                input: &json!({}),
            },
            &step.path,
            &AtomicBool::new(false),
            None,
        )
        .unwrap();
    assert!(!outcome.is_error);
    assert_eq!(
        outcome.content,
        b"Exit code: 0\nquiet result\n--- stderr ---\nloud diagnostic\n"
    );
    let dir = step.path.join(STEP_TOOLS_SUBDIR).join("tu_n");
    let output: ToolOutputRecord =
        serde_json::from_slice(&std::fs::read(dir.join(OUTPUT_FILE)).unwrap()).unwrap();
    assert_eq!(output.stderr, "loud diagnostic\n");
}

#[test]
fn stdin_payload_is_offered_verbatim_to_the_tool() {
    // §3.3 stdin: "the tool_use.input JSON object the model emitted,
    // passed verbatim". A tool that reads stdin should see exactly the
    // serialized input bytes.
    let root = HarnessRoot::new();
    root.install("echo-stdin", "cat");
    let clock = FixedClock::default();
    let step = StepDir::new();
    let exec = SpawnTool::new(root.path(), &clock, driver_target());
    let input = json!({"command": "ls -la", "n": 42});
    let outcome = exec
        .execute(
            ToolCall {
                id: "tu_s",
                name: "echo-stdin",
                input: &input,
            },
            &step.path,
            &AtomicBool::new(false),
            None,
        )
        .unwrap();
    let echoed: serde_json::Value = serde_json::from_slice(after_header(&outcome.content)).unwrap();
    assert_eq!(echoed, input);
}

#[test]
fn tool_that_ignores_stdin_still_succeeds() {
    // The §3.3 contract only requires stdin be *offered* to the tool;
    // a tool that closes stdin without reading must not surface as an
    // executor-level failure.
    let root = HarnessRoot::new();
    root.install("ignore-stdin", "echo done");
    let clock = FixedClock::default();
    let step = StepDir::new();
    let exec = SpawnTool::new(root.path(), &clock, driver_target());
    let outcome = exec
        .execute(
            ToolCall {
                id: "tu_i",
                name: "ignore-stdin",
                input: &json!("a string this time"),
            },
            &step.path,
            &AtomicBool::new(false),
            None,
        )
        .unwrap();
    assert!(!outcome.is_error);
    assert_eq!(after_header(&outcome.content), b"done\n");
}

#[test]
fn executor_sets_conv_repo_and_conv_branch_env_vars_on_tool_subprocess() {
    // Per ARCH §3.3 (env-var bullet), the executor derives the conv-repo
    // root and the calling branch from `step_dir = <conv-repo>/steps/
    // <conv-id>/<NNN>` and exports them. The dispatch built-in is the
    // v0.4 reader; this test pins the writer side so the contract can't
    // drift.
    let root = HarnessRoot::new();
    root.install(
        "echoenv",
        r#"printf "%s\n%s" "${LITANY_CONV_REPO:-}" "${LITANY_CONV_BRANCH:-}""#,
    );
    let clock = FixedClock::default();
    let step = StepDir::new();
    let exec = SpawnTool::new(root.path(), &clock, driver_target());
    let outcome = exec
        .execute(
            ToolCall {
                id: "tu_env",
                name: "echoenv",
                input: &json!({}),
            },
            &step.path,
            &AtomicBool::new(false),
            None,
        )
        .unwrap();
    assert!(!outcome.is_error);

    // step.path = <tmp>/steps/convid/001 — so the env-var derivation
    // sees conv-repo at <tmp> and conv-branch at "convid".
    let conv_repo = step
        .path
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .parent()
        .unwrap();
    let expected = format!("{}\nconvid", conv_repo.display());
    assert_eq!(
        String::from_utf8(after_header(&outcome.content).to_vec()).unwrap(),
        expected
    );

    // Sanity: pin the constant names so a rename trips this test.
    assert_eq!(ENV_CONV_REPO, "LITANY_CONV_REPO");
    assert_eq!(ENV_CONV_BRANCH, "LITANY_CONV_BRANCH");
}
