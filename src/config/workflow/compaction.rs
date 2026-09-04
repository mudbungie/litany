//! The `compaction:` block of `workflow.yaml` (ARCH §2.6, §2.7, §6):
//! checkpoint triggers and span selection, split from [`super`] to hold
//! the per-file line cap.

use crate::config::error::LoadError;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::path::Path;

/// Optional `compaction:` block (ARCH §6).
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub struct CompactionConfig {
    pub intermediate: IntermediateCompaction,
}

/// Configuration for intermediate compaction triggers.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub struct IntermediateCompaction {
    pub trigger: CompactionTrigger,
    /// Required when `trigger == every_n_commits` (commit count),
    /// `every_t_seconds` (seconds) or `window_percent` (percent of the
    /// model's context window, `1..=100`). Ignored for `on_flush`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub n: Option<u32>,
    /// Most recent commits kept out of the compaction span (ARCH §2.6):
    /// the compactor forks off `HEAD~keep_recent` — the compaction point
    /// — so the retained tail survives verbatim and replays on top of the
    /// landing. Omitted → `0`, the point is the tip. Must stay below `n`
    /// under `every_n_commits` (validated), else every landing re-arms.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub keep_recent: Option<u32>,
    /// The retained tail as **provider-reported prompt tokens** rather
    /// than commits (`docs/DESIGN_CONTEXT_ECONOMY.md` §5.2): the
    /// compaction point is the oldest model-entry commit whose usage
    /// leaves the stretch above it costing at most `n` prompt tokens to
    /// append, in the provider's own count — no tokenizer, no stored
    /// counter. One tail or the other, never both (validated).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub keep_recent_tokens: Option<u32>,
    /// Byte cap on the landing's **extract** — `summary/NNN.refs.md`, the
    /// deterministic compaction product code derives from what the
    /// compaction removes from context (ARCH §2.7,
    /// `docs/DESIGN_CONTEXT_ECONOMY.md` §5.3). Omitted → no extract is
    /// written, severable like `tool_output:`. Bytes, never tokens: the
    /// extract is a file in the tree, so the bound is stated in the unit
    /// the tree has.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub extract_bytes: Option<usize>,
}

/// Closed set of intermediate-compaction triggers (ARCH §6).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum CompactionTrigger {
    EveryNCommits,
    EveryTSeconds,
    OnFlush,
    // Due when the branch's **last usage** — the prompt side of the
    // `usage` report on the newest model entry (ARCH §2.3) — reaches `n`
    // percent of the model's context window, as the same report states
    // it (`docs/DESIGN_CONTEXT_ECONOMY.md` §5.1, and
    // `crate::prompt::compactor::checkpoint::usage` for the read and the
    // decline). A branch whose last usage carries no window is declined
    // at the boundary, never silently never-due. Stated as a plain
    // comment, not a doc comment: a per-variant description splits the
    // generated schema's flat string enum into a `oneOf`, and this set
    // is closed and flat by declaration.
    WindowPercent,
}

pub(super) fn validate_compaction(path: &Path, c: &CompactionConfig) -> Result<(), LoadError> {
    let needs_n = matches!(
        c.intermediate.trigger,
        CompactionTrigger::EveryNCommits
            | CompactionTrigger::EveryTSeconds
            | CompactionTrigger::WindowPercent
    );
    let has_n = c.intermediate.n.is_some_and(|n| n > 0);
    if needs_n && !has_n {
        return Err(LoadError::Invalid {
            path: path.to_path_buf(),
            key: "compaction.intermediate.n".into(),
            message: "must be a positive integer for the chosen trigger".into(),
        });
    }
    // `window_percent`'s `n` is a percentage, so its range is the unit's
    // own: 0 is caught above with the other missing thresholds, and
    // anything over 100 asks for a fraction of the window no usage can
    // reach — a trigger that would never fire, which is exactly what this
    // variant exists to refuse (§5.1).
    if matches!(c.intermediate.trigger, CompactionTrigger::WindowPercent)
        && c.intermediate.n.is_some_and(|n| n > 100)
    {
        return Err(LoadError::Invalid {
            path: path.to_path_buf(),
            key: "compaction.intermediate.n".into(),
            message: "window_percent's n is a percentage of the model's context \
                      window: it must be in 1..=100"
                .into(),
        });
    }
    // Two spellings of one fact: a commit count and a token budget both
    // name the retained tail, and a config declaring both states the
    // same thing twice, in units that cannot agree. Declined naming both
    // keys rather than resolved by precedence (§5.2).
    if c.intermediate.keep_recent.is_some() && c.intermediate.keep_recent_tokens.is_some() {
        return Err(LoadError::Invalid {
            path: path.to_path_buf(),
            key: "compaction.intermediate.keep_recent_tokens".into(),
            message: "declares the retained tail twice: keep_recent (commits) and \
                      keep_recent_tokens (provider-reported prompt tokens) are one \
                      fact in two units — declare one"
                .into(),
        });
    }
    let keep = c.intermediate.keep_recent.unwrap_or(0);
    if matches!(c.intermediate.trigger, CompactionTrigger::EveryNCommits)
        && c.intermediate.n.is_some_and(|n| keep >= n)
    {
        return Err(LoadError::Invalid {
            path: path.to_path_buf(),
            key: "compaction.intermediate.keep_recent".into(),
            message: "must be smaller than n: a retained tail at or over the commit \
                      trigger would re-arm the clock at every landing"
                .into(),
        });
    }
    Ok(())
}
