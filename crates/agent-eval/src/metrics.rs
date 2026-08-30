//! Per-run efficiency metrics (bl-36fa, ARCH §9.3): model attempts, tool
//! invocations, and the four canonical usage counters, derived from the
//! workspace a driver disclosed through `LITANY_EVAL_REPORT` (the same
//! two-line report `litany bundle` consumes, §9.2). The source is the
//! workspace's `steps/` slice — the same tree the harness's own budget
//! derivation reads (ARCH §6, §8) — parsed with brazen's `v=1` event
//! vocabulary, so the runner and the harness cannot disagree about what
//! an attempt segment or a usage counter is.
//!
//! **Missing is never zero.** A run whose driver disclosed no workspace
//! has no metrics at all (`Option` at the [`crate::record::RunRecord`]
//! grain); a provider that never reported a usage counter leaves it
//! `None` (brazen's own rule: `0` would be a lie). Nothing here infers
//! price or fabricates token counts — the runner has no tokenizer.

use crate::record::RunRecord;
use brazen::{ContentKind, Event};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::Path;

/// Workspace subdir holding per-agent step records (ARCH §2.2).
const STEPS_DIR: &str = "steps";
/// Per-step JSONL of `v=1` events (ARCH §2.3, §4.4).
const RESPONSE_FILE: &str = "response.json";
/// Zero-padded step-sequence width (`001`, `002`, …) per ARCH §2.3.
const STEP_SEQ_WIDTH: usize = 3;

/// What one disclosed run cost, read off its workspace's `steps/` slice.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct RunMetrics {
    /// Attempt segments across every step (≡ API attempts per model
    /// call, ARCH §4.4: one segment per adapter invocation, each
    /// terminated by `end`).
    pub attempts: u64,
    /// `tool_use` blocks in each step's authoritative segment — the last
    /// complete one (§4.4 segment authority) — i.e. the tool invocations
    /// the harness actually executed.
    pub tool_invocations: u64,
    /// The four canonical usage counters (brazen `Usage`), each summed
    /// across *every* attempt segment (§6: failed and superseded
    /// attempts are billed). `None` = the provider never reported it.
    pub input_tokens: Option<u64>,
    pub output_tokens: Option<u64>,
    pub cache_read_tokens: Option<u64>,
    pub cache_write_tokens: Option<u64>,
    /// Model ids observed in `message_start` events (sorted, deduped).
    pub models: Vec<String>,
    /// Provider rows the observed models resolve to through the run
    /// home's `models.yaml` (sorted, deduped; empty when unresolvable).
    pub providers: Vec<String>,
}

/// Derive one run's metrics from the disclosed `workspace` — walking
/// `steps/<agent_id>/` and every `steps/<agent_id>-*/` hyphen-descendant
/// (ARCH §2.2 descent, the same walk as the harness budget derivation)
/// — and resolve providers through `<litany_home>/models.yaml`. A
/// missing or partial tree contributes nothing; it never errors.
pub fn collect(workspace: &Path, agent_id: &str, litany_home: &Path) -> RunMetrics {
    let mut m = RunMetrics::default();
    let mut models = BTreeSet::new();
    let prefix_dash = format!("{agent_id}-");
    if let Ok(entries) = fs::read_dir(workspace.join(STEPS_DIR)) {
        for entry in entries.flatten() {
            let raw = entry.file_name();
            let name = raw.to_string_lossy();
            if name == agent_id || name.starts_with(&prefix_dash) {
                fold_agent_dir(&mut m, &mut models, &entry.path());
            }
        }
    }
    m.models = models.into_iter().collect();
    m.providers = providers_for(&m.models, &litany_home.join("models.yaml"));
    m
}

/// Fold every 3-digit step subdir of one agent-id dir into `m`.
fn fold_agent_dir(m: &mut RunMetrics, models: &mut BTreeSet<String>, agent_dir: &Path) {
    let Ok(entries) = fs::read_dir(agent_dir) else {
        return;
    };
    for entry in entries.flatten() {
        let raw = entry.file_name();
        let name = raw.to_string_lossy();
        if name.len() == STEP_SEQ_WIDTH && name.bytes().all(|b| b.is_ascii_digit()) {
            fold_step(m, models, &entry.path().join(RESPONSE_FILE));
        }
    }
}

/// Fold one step's `response.json` into `m`: attempts (`end` count),
/// tool invocations (authoritative segment only), usage (every
/// segment), and observed models. Malformed or forward-compat lines are
/// skipped (the `v=1` tolerate-unknown contract, §4.4).
fn fold_step(m: &mut RunMetrics, models: &mut BTreeSet<String>, response: &Path) {
    let Ok(bytes) = fs::read(response) else {
        return;
    };
    let events: Vec<Event> = bytes
        .split(|b| *b == b'\n')
        .filter(|line| !line.is_empty())
        .filter_map(|line| serde_json::from_slice::<Event>(line).ok())
        .collect();
    let ends: Vec<usize> = events
        .iter()
        .enumerate()
        .filter_map(|(i, e)| matches!(e, Event::End).then_some(i))
        .collect();
    m.attempts += ends.len() as u64;
    if let Some(&last) = ends.last() {
        let start = match ends.len().checked_sub(2) {
            Some(i) => ends[i] + 1,
            None => 0,
        };
        m.tool_invocations += events[start..last]
            .iter()
            .filter(|e| {
                matches!(
                    e,
                    Event::ContentStart {
                        kind: ContentKind::ToolUse { .. },
                        ..
                    }
                )
            })
            .count() as u64;
    }
    for event in &events {
        match event {
            Event::Usage(u) => {
                add(&mut m.input_tokens, u.input_tokens);
                add(&mut m.output_tokens, u.output_tokens);
                add(&mut m.cache_read_tokens, u.cache_read_tokens);
                add(&mut m.cache_write_tokens, u.cache_write_tokens);
            }
            Event::MessageStart {
                model: Some(model), ..
            } => {
                models.insert(model.clone());
            }
            _ => {}
        }
    }
}

/// Fold one reported counter into an accumulator. A reported value makes
/// the total reported; an unreported one leaves it as it was — so the
/// total is `None` iff *no* usage event ever carried the counter.
fn add(acc: &mut Option<u64>, v: Option<u32>) {
    if let Some(v) = v {
        *acc = Some(acc.unwrap_or(0) + u64::from(v));
    }
}

/// The subset of the run home's `models.yaml` (ARCH §4.2) this crate
/// reads: model key → provider row (extra fields tolerated).
#[derive(Deserialize)]
struct ModelsFile {
    #[serde(default)]
    models: BTreeMap<String, ModelRow>,
}

#[derive(Deserialize)]
struct ModelRow {
    #[serde(default)]
    provider: Option<String>,
    #[serde(default)]
    model_id: Option<String>,
}

/// Resolve observed model ids to brazen provider-row names via the run
/// home's `models.yaml` (matching the entry key or its `model_id`). An
/// absent or unparseable file, or an unmatched model, resolves to
/// nothing — reported as unresolved, never guessed.
fn providers_for(observed: &[String], models_yaml: &Path) -> Vec<String> {
    let Ok(text) = fs::read_to_string(models_yaml) else {
        return Vec::new();
    };
    let Ok(file) = serde_yaml_ng::from_str::<ModelsFile>(&text) else {
        return Vec::new();
    };
    let mut rows = BTreeSet::new();
    for model in observed {
        for (key, row) in &file.models {
            if (key == model || row.model_id.as_deref() == Some(model))
                && let Some(provider) = &row.provider
            {
                rows.insert(provider.clone());
            }
        }
    }
    rows.into_iter().collect()
}

/// Aggregate efficiency over a set of runs — the grain the report and
/// the baseline→candidate comparison both render. `wall_ms` covers all
/// runs (the runner measures it itself); the derived counters cover
/// only runs whose driver disclosed a workspace, and are `None` when no
/// run did (missing, not zero).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Efficiency {
    pub runs: usize,
    pub disclosed: usize,
    pub wall_ms: u64,
    pub attempts: Option<u64>,
    pub tool_invocations: Option<u64>,
    pub input_tokens: Option<u64>,
    pub output_tokens: Option<u64>,
    pub cache_read_tokens: Option<u64>,
    pub cache_write_tokens: Option<u64>,
}

impl Efficiency {
    /// Fold a set of runs into one aggregate.
    pub fn over<'a>(runs: impl IntoIterator<Item = &'a RunRecord>) -> Self {
        let mut e = Efficiency::default();
        for run in runs {
            e.runs += 1;
            e.wall_ms += run.wall_ms;
            if let Some(m) = &run.metrics {
                e.disclosed += 1;
                e.attempts = Some(e.attempts.unwrap_or(0) + m.attempts);
                e.tool_invocations = Some(e.tool_invocations.unwrap_or(0) + m.tool_invocations);
                fold(&mut e.input_tokens, m.input_tokens);
                fold(&mut e.output_tokens, m.output_tokens);
                fold(&mut e.cache_read_tokens, m.cache_read_tokens);
                fold(&mut e.cache_write_tokens, m.cache_write_tokens);
            }
        }
        e
    }

    /// Mean outer wall seconds per run (0 over zero runs).
    pub fn wall_mean_s(&self) -> f64 {
        if self.runs == 0 {
            return 0.0;
        }
        self.wall_ms as f64 / self.runs as f64 / 1000.0
    }

    /// Mean attempts per disclosed run; `None` when nothing disclosed.
    pub fn attempts_mean(&self) -> Option<f64> {
        self.attempts.map(|a| a as f64 / self.disclosed as f64)
    }

    /// Mean tool invocations per disclosed run; `None` when nothing
    /// disclosed.
    pub fn tools_mean(&self) -> Option<f64> {
        self.tool_invocations
            .map(|t| t as f64 / self.disclosed as f64)
    }
}

/// [`add`] for already-widened totals.
fn fold(acc: &mut Option<u64>, v: Option<u64>) {
    if let Some(v) = v {
        *acc = Some(acc.unwrap_or(0) + v);
    }
}
