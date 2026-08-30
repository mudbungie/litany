//! `workflow.yaml`'s `tool_output:` block — the **bounded projection**
//! policy (ARCH §3.3 *Bounded transcript projection*).
//!
//! A tool's full stdout/stderr always lands in the diagnostic
//! `steps/<agent-id>/<NNN>/tools/<tool-id>/output.json` (§3.3 Disk
//! record); this block bounds only the *transcript projection* — the
//! bytes committed as the `tool_result` entry the model reads on every
//! later step. Each stream is bounded independently to its first
//! `head_bytes` and last `tail_bytes`, the omitted middle replaced by a
//! marker stating what was cut and where the full record lives.
//!
//! Like every other file in this module, this is policy severable from
//! mechanism: the shipped default lives in `template/workflow.yaml`,
//! and omitting the block leaves tool output unbounded — the general
//! path with the policy absent, not a distinct code path.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// The `tool_output:` block. Both fields are byte counts — litany has
/// no tokenizer, so bytes are the only honest unit (§3.3). `Copy`
/// because it is two words of policy handed down the executor path by
/// value.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ToolOutputBound {
    /// Bytes kept from the start of each stream — the command banner,
    /// the part that says what ran.
    pub head_bytes: usize,
    /// Bytes kept from the end of each stream — the failure tail, the
    /// part that says how it ended.
    pub tail_bytes: usize,
}
