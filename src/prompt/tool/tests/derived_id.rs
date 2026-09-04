//! A **derived inner id** through the executor (ARCH §3.3 env-var
//! bullet, *The program*): an inner invocation runs under
//! `<outer-id>-<k>`, and `LITANY_TOOL_ID` is set from the same
//! `ToolCall` the record directory is named after — so a tool reads the
//! id of the directory its own record landed in, which is what lets the
//! `python` built-in serve its stub module from beside its record
//! rather than being told where it is.
//!
//! This is the surviving half of the batch suite: the executor's
//! `execute_all` retired with the `parallel` multi-tool envelope
//! (`docs/DESIGN_CODE_EXECUTION.md` §5), and a program fans with a
//! thread pool over its own stub module instead.

use super::super::{
    OUTPUT_FILE, STEP_TOOLS_SUBDIR, SpawnTool, ToolCall, ToolExecutor, ToolOutputRecord,
};
use super::fixtures::{FixedClock, HarnessRoot, StepDir, driver_target};
use serde_json::json;
use std::sync::atomic::AtomicBool;

#[test]
fn each_inner_invocation_reads_its_own_derived_tool_id() {
    let root = HarnessRoot::new();
    root.install("whoami", r#"printf "%s" "${LITANY_TOOL_ID:-}""#);
    let clock = FixedClock::default();
    let step = StepDir::new();
    let exec = SpawnTool::new(root.path(), &clock, driver_target());
    let input = json!({});
    for id in ["toolu_outer-1", "toolu_outer-2"] {
        let outcome = exec
            .execute(
                ToolCall {
                    id,
                    name: "whoami",
                    input: &input,
                },
                &step.path,
                &AtomicBool::new(false),
                None,
            )
            .expect("the tool ran");
        assert_eq!(outcome.content, format!("Exit code: 0\n{id}").into_bytes());
        let dir = step.path.join(STEP_TOOLS_SUBDIR).join(id);
        let record: ToolOutputRecord =
            serde_json::from_slice(&std::fs::read(dir.join(OUTPUT_FILE)).unwrap()).unwrap();
        assert_eq!(record.stdout, id, "the tool read the id of its own record");
    }
}
