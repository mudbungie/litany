//! `<conv-repo>/workflow.yaml` — event-to-action bindings per ARCH §6.
//!
//! Workflows are declarative: events are drawn from a closed set, actions
//! are drawn from another closed set (defined in [`crate::config::action`]).
//! Adding either is intentionally a code change.

use crate::config::action::Action;
use crate::config::error::LoadError;
use crate::config::tool_control::ToolControl;
use crate::config::tool_output::ToolOutputBound;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::Path;

pub mod compaction;
use compaction::validate_compaction;
pub use compaction::{CompactionConfig, CompactionTrigger};

/// Top-level `workflow.yaml` shape.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub struct Workflow {
    pub events: BTreeMap<Event, Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub compaction: Option<CompactionConfig>,
    /// Retry policy for a step's model call (ARCH §2.10, §4.4): the
    /// harness owns the retry loop (brazen never retries), reading the
    /// attempt cap and backoff from here. Omitted uses [`RetryConfig::
    /// default`].
    #[serde(default)]
    pub retry: RetryConfig,
    /// Whole-tree spend limits (ARCH §6 "Budgets (v0.7)"). Checked
    /// at every model-call boundary before invoking the adapter; every
    /// value is derived at check time from on-disk `Usage` events, step
    /// timestamps, and branch depth — the harness stores no running
    /// counter (PRINCIPLES "Single source of truth"). Omitted → every
    /// limit unbounded, which is what ships: `template/workflow.yaml`
    /// declares no `budgets:` block at all (ARCH §6 "Nothing ships
    /// bounded", operator ruling 2026-08-16), so declaring a ceiling is
    /// config an operator adds and removing one deletes config, never
    /// code.
    #[serde(default)]
    pub budgets: Budgets,
    /// Per-stream byte bound on the transcript projection of a tool
    /// result (ARCH §3.3 *Bounded transcript projection*, bl-d5fa).
    /// Omitted → tool output reaches the transcript unbounded; the
    /// shipped default lives in `template/workflow.yaml`
    /// ([`ToolOutputBound`]).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_output: Option<ToolOutputBound>,
    /// The `context_files:` list (ARCH §3.3 *Context files ride the
    /// next tool result*, `docs/DESIGN_CONTEXT_ECONOMY.md` §6): file
    /// **names** looked for in every directory on the path from the
    /// enclosing repository's top level down to the agent's working
    /// directory, each carried once on a tool result. Empty — the block
    /// omitted — discovers nothing, which is the general path with the
    /// policy absent; the shipped list is `template/workflow.yaml`'s.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub context_files: Vec<String>,
    /// The tool-control seam (ARCH §3.3 *Tool control*, §6): the
    /// adjudicator consulted before every granted tool invocation
    /// executes — pass, refuse, or hold. Omitted → no control is
    /// consulted and the tool window is unchanged; no control is
    /// shipped ([`ToolControl`]).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_control: Option<ToolControl>,
}

/// Whole-tree spend limits (ARCH §6 `budgets:` block, v0.7). Each
/// limit is optional; an omitted limit is unbounded. All three are derived
/// live from disk at check time — never stored — and are a whole-tree
/// ceiling: any driver (root or subagent) checks the tree's total spend
/// against the single frozen limit, with no per-dispatch inheritance
/// (ARCH §6; the tree shares one `steps/` per §2.2/§2.3).
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub struct Budgets {
    /// Cap on tokens summed across *every* attempt segment of every
    /// step's `response.json` in the conversation tree — failed and
    /// superseded attempts are billed too (ARCH §6/§8; the
    /// last-segment-authoritative rule governs *context*, not billing).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_total_tokens: Option<u64>,
    /// Cap on wall-clock seconds summed per step from `meta.json`'s
    /// `started_at`→`ended_at`; each span already includes the backoff
    /// sleeps between that step's attempts (ARCH §6 "wall is wall",
    /// §2.10).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_wall_seconds: Option<u64>,
    /// The deepest *allowed* dispatch depth (root agent = 0; each
    /// dispatch is one deeper). An agent deeper than this exhausts on
    /// its first model call, and a dispatch that would land a child
    /// deeper is refused before the fork (ARCH §6 "The depth boundary").
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_depth: Option<u32>,
}

/// Harness-owned retry policy (ARCH §6 `retry:` block). One `bz`
/// process per attempt (§4.4); a retryable in-band `Error` re-invokes
/// `bz` with the identical request up to `max_attempts`, sleeping the
/// backoff between attempts (§2.10).
#[derive(Debug, Clone, Copy, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub struct RetryConfig {
    /// Attempt cap per model call — `1` disables retry (a single
    /// attempt). Each attempt is one `bz` invocation appending one
    /// segment to `response.json` (§4.4).
    pub max_attempts: u32,
    pub backoff: Backoff,
}

impl Default for RetryConfig {
    fn default() -> Self {
        // Matches the ARCH §6 example: 3 attempts, exponential backoff.
        Self {
            max_attempts: 3,
            backoff: Backoff::Exponential,
        }
    }
}

/// Closed set of backoff policies between retry attempts (ARCH §6).
/// Exponential is the shipped policy; adding another is a code change,
/// like every other closed workflow set.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Backoff {
    Exponential,
}

/// Base delay before the first retry (the exponential's first rung).
const BACKOFF_BASE_MS: u64 = 250;

impl Backoff {
    /// Effective delay before the retry that follows a failed `attempt`
    /// (1-based): the config schedule, floored by the provider's pacing
    /// hint (ARCH §4.4). Exponential doubles per rung from
    /// [`BACKOFF_BASE_MS`], saturating so a pathological attempt count
    /// cannot overflow.
    ///
    /// `retry_after_seconds` is the failed attempt's
    /// `CanonicalError::retry_after_seconds` — the provider's
    /// `Retry-After` header in whole seconds. It is a **floor, never a
    /// shrink**: a hint below the schedule changes nothing, a hint above
    /// it wins, and `None` (the header absent or unparseable) leaves the
    /// schedule to govern alone.
    pub fn delay(self, attempt: u32, retry_after_seconds: Option<u32>) -> std::time::Duration {
        let scheduled = match self {
            Backoff::Exponential => {
                let factor = 2u64.saturating_pow(attempt.saturating_sub(1));
                std::time::Duration::from_millis(BACKOFF_BASE_MS.saturating_mul(factor))
            }
        };
        let hint = retry_after_seconds.map_or(std::time::Duration::ZERO, |s| {
            std::time::Duration::from_secs(u64::from(s))
        });
        scheduled.max(hint)
    }
}

/// Closed set of workflow events. Names match the arch examples.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum Event {
    UserMessage,
    WorkerReturn,
    VerifierApprove,
    VerifierReject,
    WorkerFlush,
    CompactorReturn,
    /// A reviewer child's return (`docs/DESIGN_LEARNING_LOOP.md` §3),
    /// derived from the child's dispatch-commit role exactly as
    /// [`Event::CompactorReturn`] is. Bound to `stage_proposal` in the
    /// seeded `learning-loop.yaml`; no shipped config in the basic
    /// agentic loop binds it, and an unbound event is the empty-inputs
    /// no-op.
    ReviewerReturn,
    BranchStopped,
    PreStep,
    PostStep,
    OnToolReturn,
}

impl Event {
    /// The `workflow.yaml` key for this event (ARCH §6) — the stable name
    /// used in diagnostics and by the runtime interpreter.
    pub fn as_str(self) -> &'static str {
        event_name(self)
    }
}

impl Workflow {
    /// Parse and validate workflow YAML already in hand — the
    /// governing-config read path (ARCH §2.2: control is read from the
    /// config commit's tree, never from a worktree file). `origin`
    /// labels errors (e.g. `<config-commit>:workflow.yaml`).
    pub fn parse(raw: &str, origin: &Path) -> Result<Self, LoadError> {
        let parsed: Self = serde_yaml_ng::from_str(raw).map_err(|source| LoadError::Yaml {
            path: origin.to_path_buf(),
            source,
        })?;
        parsed.validate(origin)?;
        Ok(parsed)
    }

    fn validate(&self, path: &Path) -> Result<(), LoadError> {
        for (event, actions) in &self.events {
            for (i, raw) in actions.iter().enumerate() {
                Action::parse(raw).map_err(|message| LoadError::Invalid {
                    path: path.to_path_buf(),
                    key: format!("events.{}[{i}]", event_name(*event)),
                    message,
                })?;
            }
        }
        if let Some(compaction) = &self.compaction {
            validate_compaction(path, compaction)?;
        }
        if let Some(control) = &self.tool_control
            && control.command.trim().is_empty()
        {
            return Err(LoadError::Invalid {
                path: path.to_path_buf(),
                key: "tool_control.command".into(),
                message: "must name the control executable (ARCH §3.3 Tool control)".into(),
            });
        }
        Ok(())
    }

    /// The typed actions bound to one `event`, in declared order (ARCH §6
    /// "The binding interpreter" — the flat list the hop matches against
    /// disk circumstance). An unbound event yields the empty list: the
    /// general path with empty inputs, not a bootstrap special case.
    /// Strings were validated at load, so parsing here cannot fail.
    pub fn actions_for(&self, event: Event) -> Vec<Action> {
        self.events
            .get(&event)
            .map(|raw| {
                raw.iter()
                    .map(|s| Action::parse(s).expect("validated at load"))
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Pre-parse every action string into a typed [`Action`], keyed by
    /// event — the whole-workflow view [`crate::config::cross::
    /// check_workflow_against_roles`] sweeps at config load (§4.3), as
    /// against [`actions_for`](Self::actions_for)'s single-event lookup on
    /// the §6 hot path. Strings were validated at parse, so this cannot
    /// fail.
    pub fn typed_events(&self) -> BTreeMap<Event, Vec<Action>> {
        self.events
            .iter()
            .map(|(event, actions)| {
                let parsed = actions
                    .iter()
                    .map(|raw| Action::parse(raw).expect("validated at load"))
                    .collect();
                (*event, parsed)
            })
            .collect()
    }
}

fn event_name(event: Event) -> &'static str {
    match event {
        Event::UserMessage => "user_message",
        Event::WorkerReturn => "worker_return",
        Event::VerifierApprove => "verifier_approve",
        Event::VerifierReject => "verifier_reject",
        Event::WorkerFlush => "worker_flush",
        Event::CompactorReturn => "compactor_return",
        Event::ReviewerReturn => "reviewer_return",
        Event::BranchStopped => "branch_stopped",
        Event::PreStep => "pre_step",
        Event::PostStep => "post_step",
        Event::OnToolReturn => "on_tool_return",
    }
}

// Tests for `workflow.yaml` parsing live in `tests/workflow_yaml.rs`.
