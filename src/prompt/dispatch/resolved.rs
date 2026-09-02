//! The resolved shape one step loop runs against (ARCH §4.2, §4.3).
//!
//! Held apart from [`super`]'s orchestration because it is data, not
//! sequence: `litany prompt` and the `litany advance` hop build the same
//! value from their own config sources (§6 *one struct, two drivers*)
//! and the loop reads it without knowing which resolved it.

use super::Grant;
use crate::config::manifest::RoleRules;
use crate::config::{Budgets, Effort, RetryConfig, Workflow};
use std::ffi::OsString;

/// Inputs resolved by [`super::run`] before branch work starts.
pub(in crate::prompt) struct Resolved<'a> {
    /// The agent's role (§4.3), its `tools:` grant and the config commit
    /// all three were read from (§2.2) — one value, because the role
    /// governs what the request declares ([`tools::compose`]) and may
    /// call ([`tool_step`]) *and* the grant selects the descriptors the
    /// dispatch commit derives from that commit (§3.3).
    pub(in crate::prompt) grant: Grant<'a>,
    /// The model id the role's assignment names (§4.3) — rides the
    /// canonical request verbatim; validity is brazen's fact (§4.2).
    pub(in crate::prompt) model_id: &'a str,
    /// brazen provider-row name passed as `bz --provider <row>` (§4.4).
    pub(in crate::prompt) provider_row: &'a str,
    /// The role's reasoning-effort level (§4.3 `effort:`) — rides every
    /// model call's `reasoning` knob; `None` leaves it unset.
    pub(in crate::prompt) effort: Option<Effort>,
    /// Whether the role asks the provider's priority lane (§4.3
    /// `priority:`) — rides every model call's `service_tier` knob.
    /// Unset and `false` are one fact: the knob stays absent.
    pub(in crate::prompt) priority: Option<bool>,
    pub(in crate::prompt) soul: String,
    /// The adapter binary (`bz` or the `adapter:` override, §4.2).
    pub(in crate::prompt) binary: OsString,
    /// Harness-owned retry policy from `workflow.yaml` (§2.10, §6).
    pub(in crate::prompt) retry: RetryConfig,
    /// Whole-tree spend limits from `workflow.yaml` (§6), checked
    /// at every model-call boundary.
    pub(in crate::prompt) budgets: Budgets,
    /// The full workflow (§6): per-step hooks and lifecycle bindings, the
    /// same seams `litany advance` runs (the §6 prompt→advance collapse).
    pub(in crate::prompt) workflow: &'a Workflow,
    /// The role's §5.2 context-assembly rules (`manifest.yaml`, §2.2);
    /// `None` (no entry for the role) assembles the transcript alone.
    pub(in crate::prompt) manifest: Option<&'a RoleRules>,
    /// True under an `adapter:` override — the MessageStart.v handshake
    /// governs in place of the version guard (§4.4).
    pub(in crate::prompt) expect_handshake: bool,
}
