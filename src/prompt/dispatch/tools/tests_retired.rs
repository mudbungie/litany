//! A **retired** tool that an agent's own history still names
//! (`docs/DESIGN_CODE_EXECUTION.md` §5): `multi_tool` shipped a schema,
//! a skill and a grant until the program replaced it, so transcripts
//! that carry a `multi_tool` `tool_use` exist and are immutable (ARCH
//! §2.3).
//!
//! Two halves, and they pull in opposite directions on purpose. The
//! request must still **declare** the name, or the provider refuses the
//! whole exchange the history already contains (§3.3 — the array is
//! closed over the history it ships); and the role must not be able to
//! **call** it, which is the ordinary grant gate, tested at the door
//! (`cmd::tests::invoking_gates`). Declaring is not permitting, and a
//! retirement is the sharpest case of it.

use super::tests::{custom, history_calling};
use super::*;
use serde_json::json;
use tempfile::TempDir;

#[test]
fn a_transcript_that_names_the_retired_multi_tool_still_assembles() {
    // No `descriptions/tools/multi_tool.json` is committed anywhere any
    // more — the pool stopped shipping one — so the closure stands in
    // the bare object schema, exactly as for a name a model invented.
    let wt = TempDir::new().unwrap();
    let tools = compose(wt.path(), &[], &history_calling(&["multi_tool"]), &[]).unwrap();

    assert_eq!(tools.len(), 1);
    let (name, description, input_schema) = custom(&tools[0]);
    assert_eq!(name, "multi_tool");
    assert_eq!(description, None);
    assert_eq!(*input_schema, json!({"type":"object"}));
}
