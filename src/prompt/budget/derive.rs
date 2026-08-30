//! On-disk budget derivations (ARCH §6, §8): spend, wall, and depth for
//! a conversation and its descent. Every value is a pure function of the
//! `<conv-repo>/steps/` tree (or the branch name) at read time — the
//! harness stores no running counter (PRINCIPLES "Single source of
//! truth"). Callers re-derive on every check.
//!
//! **Branch and its descent.** A conversation's spend and wall span its
//! own `steps/<branch>/` records *and* every descended subagent's
//! `steps/<branch>-*/` (hyphenated descent, ARCH §2.2) — the same prefix
//! walk `litany stop` uses to cascade a stop (§2.9).

use crate::prompt::step::StepMeta;
use brazen::Event;
use std::fs;
use std::path::Path;

/// Conv-repo subdir holding per-conversation step records (ARCH §2.2).
const STEPS_DIR: &str = "steps";
/// Per-step JSONL of `v=1` events (ARCH §2.3, §4.4).
const RESPONSE_FILE: &str = "response.json";
/// Per-step metadata carrying the `started_at`/`ended_at` span (§2.3).
const META_FILE: &str = "meta.json";
/// Zero-padded step-sequence width (`001`, `002`, …) per ARCH §2.3.
const STEP_SEQ_WIDTH: usize = 3;

/// Sum `Usage` tokens across *every* attempt segment of *every*
/// `response.json` under `branch` and its descent (ARCH §6 "Every
/// attempt segment counts": failed and superseded attempts are billed).
/// A `None` counter contributes 0 — never a fabricated value.
pub fn spend(repo: &Path, branch: &str) -> u64 {
    sum_over_descent(repo, branch, step_tokens)
}

/// Wall-clock seconds summed per step from `meta.json`'s
/// `started_at`→`ended_at` across `branch` and its descent. Each span
/// already covers the backoff sleeps between that step's attempts (ARCH
/// §2.10, §4.4 "Fd held open for the whole model call"), so wall counts
/// sleeping as well as streaming (§6 "wall is wall").
pub fn wall_seconds(repo: &Path, branch: &str) -> u64 {
    sum_over_descent(repo, branch, step_wall)
}

/// Dispatch depth of `branch`: a root agent is one `<ts>-<short>` id
/// (one hyphen) at depth 0; each dispatch appends `-<ts>-<short>` (two
/// more hyphens), so depth = hyphens / 2 (ARCH §6 "The depth boundary",
/// over the §2.3 hyphenated descent). Relies on the id token format —
/// the compact timestamp and short id are both hyphen-free (clock.rs /
/// ARCH §2.3).
pub fn depth(branch: &str) -> u32 {
    (branch.matches('-').count() / 2) as u32
}

/// Walk `steps/<branch>/` and every `steps/<branch>-*/` conv-id dir,
/// folding `per_step` over each 3-digit step subdir and summing. A
/// missing `steps/` tree (or an entry that is not a readable directory)
/// contributes 0 — the derivation never panics on a partial tree.
fn sum_over_descent(repo: &Path, branch: &str, per_step: fn(&Path) -> u64) -> u64 {
    let Ok(entries) = fs::read_dir(repo.join(STEPS_DIR)) else {
        return 0;
    };
    let prefix_dash = format!("{branch}-");
    let mut total = 0u64;
    for entry in entries.flatten() {
        let raw = entry.file_name();
        let name = raw.to_string_lossy();
        if name != branch && !name.starts_with(&prefix_dash) {
            continue;
        }
        total = total.saturating_add(sum_conv_steps(&entry.path(), per_step));
    }
    total
}

/// Fold `per_step` over the 3-digit step subdirs of one conv-id dir.
/// A conv-id entry that is not a readable directory contributes 0.
fn sum_conv_steps(conv_dir: &Path, per_step: fn(&Path) -> u64) -> u64 {
    let Ok(entries) = fs::read_dir(conv_dir) else {
        return 0;
    };
    let mut total = 0u64;
    for entry in entries.flatten() {
        let raw = entry.file_name();
        let name = raw.to_string_lossy();
        if name.len() == STEP_SEQ_WIDTH && name.bytes().all(|b| b.is_ascii_digit()) {
            total = total.saturating_add(per_step(&entry.path()));
        }
    }
    total
}

/// Sum `Usage` tokens over every event line of one step's
/// `response.json`. A missing file contributes 0; a malformed or
/// forward-compat line is skipped (the `v=1` tolerate-unknown contract,
/// §4.4). Every `Usage` line across every segment is counted (§6).
fn step_tokens(step_dir: &Path) -> u64 {
    let Ok(bytes) = fs::read(step_dir.join(RESPONSE_FILE)) else {
        return 0;
    };
    bytes
        .split(|b| *b == b'\n')
        .filter(|line| !line.is_empty())
        .filter_map(|line| serde_json::from_slice::<Event>(line).ok())
        .map(usage_tokens)
        .sum()
}

/// One `Usage` event's tokens, folded so a cached slice that already
/// sits INSIDE the prompt counter is billed once (ARCH §6 "The cached
/// slice is billed once"): `max(input, cache_read + cache_write) +
/// output`.
///
/// brazen's canonical `Usage` reports each provider's own counters
/// unaltered (brazen `specs/architecture.md` §3.2), and providers
/// disagree about overlap: Anthropic's prompt counters are disjoint
/// slices (`input_tokens` beside `cache_read_input_tokens` /
/// `cache_creation_input_tokens`), while the OpenAI-shaped and Google
/// decoders map a prompt counter that CONTAINS the cached one
/// (`prompt_tokens` ⊇ `prompt_tokens_details.cached_tokens`,
/// `input_tokens` ⊇ `input_tokens_details.cached_tokens`,
/// `promptTokenCount` ⊇ `cachedContentTokenCount`). Nothing on the
/// `Usage` event says which shape it is, and a step record carries no
/// protocol, so the fold takes the larger of the two readings of the
/// prompt rather than their sum: exact where the slice is contained,
/// a floor (never an over-statement) where the counters are disjoint,
/// and plain `input + output` where no cache counter is reported.
///
/// A floor rather than a ceiling on purpose — spend is what was really
/// consumed, and billing a prompt twice ends a conversation before the
/// ceiling its operator declared. Collapses back to the plain sum if
/// brazen ever guarantees disjoint slices (litany bl-68f5 / brazen
/// bl-d192). Each `None` field is 0; non-`Usage` events carry no tokens.
fn usage_tokens(event: Event) -> u64 {
    match event {
        Event::Usage(u) => {
            let cached = opt(u.cache_read_tokens) + opt(u.cache_write_tokens);
            opt(u.input_tokens).max(cached) + opt(u.output_tokens)
        }
        _ => 0,
    }
}

fn opt(v: Option<u32>) -> u64 {
    v.map(u64::from).unwrap_or(0)
}

/// The `started_at`→`ended_at` span of one step (seconds). A missing or
/// malformed `meta.json`, or an unparseable timestamp, contributes 0.
fn step_wall(step_dir: &Path) -> u64 {
    let Ok(bytes) = fs::read(step_dir.join(META_FILE)) else {
        return 0;
    };
    let Ok(meta) = serde_json::from_slice::<StepMeta>(&bytes) else {
        return 0;
    };
    span_seconds(&meta.started_at, &meta.ended_at)
}

/// Seconds from `start` to `end` (RFC3339 / ISO-8601 — the clock.rs
/// format). An unparseable pair, or `end` before `start`, is 0 —
/// defensive against a partially-written or malformed record.
fn span_seconds(start: &str, end: &str) -> u64 {
    use chrono::DateTime;
    let (Ok(s), Ok(e)) = (
        DateTime::parse_from_rfc3339(start),
        DateTime::parse_from_rfc3339(end),
    ) else {
        return 0;
    };
    (e - s).num_seconds().max(0) as u64
}
