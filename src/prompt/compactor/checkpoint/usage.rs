//! **The branch's last usage** — the provider's token report on the
//! newest model entry of the read-state tree, and the `window_percent`
//! predicate over it (`docs/DESIGN_CONTEXT_ECONOMY.md` §5.1).
//!
//! Usage rides the transcript entry (ARCH §2.3 *Usage rides the entry*),
//! so the window trigger reads the tree the state derivation already
//! holds — `messages/NNN-<model-id>.json`, the same listing context
//! assembly walks — and never `steps/`, which is diagnostic-only (§2.3).
//!
//! **Both numbers are the provider's, recorded and never computed.** The
//! numerator is the entry's *prompt side*: `input_tokens` plus
//! `cache_read_tokens` plus `cache_write_tokens`, each absent counter
//! contributing nothing, because a `0` for a counter the provider never
//! stated would be litany's arithmetic wearing brazen's voice
//! ([`crate::prompt::dispatch::entry`], brazen's zero-vs-unknown rule).
//! The denominator is the `context_window` the same report carries —
//! brazen states it in band on the `Usage` event, and the transcript
//! writer folds it in beside the counters like any other field brazen
//! adds under `v=1`, so litany keeps no per-model table (ARCH §4.2).
//!
//! **An unknown window is declined, loudly.** A workflow that names
//! `window_percent` for a model whose report carries no window has asked
//! for a threshold nothing can be measured against; answering "not due"
//! would ship a trigger that silently never fires, so the boundary
//! refuses instead, naming the model (`docs/PRINCIPLES.md`, decline
//! illegal operations). A branch with **no model entry at all** — step 1,
//! before its first model call — is simply not due: that is the general
//! path with empty inputs, not an unknown window.

use super::Error;
use serde_json::Value;
use std::path::Path;

/// Branch-scoped transcript directory (ARCH §2.3 — `messages/NNN-…`).
pub(super) const MESSAGES_DIR: &str = "messages";
/// The one reserved `.json` origin token (§2.3): a `tool` entry is a
/// `tool_result`, not model output, and carries no usage report. Every
/// other `.json` token is the model id that authored the entry.
const TOOL_ORIGIN: &str = "tool";

/// The provider's report on the branch's newest model entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LastUsage {
    /// The prompt side as reported: `input_tokens + cache_read_tokens +
    /// cache_write_tokens`, absent counters contributing nothing.
    pub prompt_tokens: u64,
    /// The model's context window as the same report states it; `None`
    /// when the provider could not state one (module docs).
    pub context_window: Option<u64>,
    /// The entry's origin token — the model that authored it (§2.3,
    /// §4.3). Carried so a decline can name the model whose window is
    /// unknown rather than the branch.
    pub model: String,
}

/// The `window_percent` predicate (§5.1): due when `last`'s prompt side
/// reaches `n` percent of its reported context window. No last usage is
/// not due; a last usage with no window is the decline.
pub(super) fn due(n: Option<u32>, last: Option<&LastUsage>) -> Result<bool, Error> {
    let (Some(n), Some(last)) = (n.filter(|n| *n > 0), last) else {
        return Ok(false);
    };
    let Some(window) = last.context_window else {
        return Err(Error::CompactionWindowUnknown {
            model: last.model.clone(),
        });
    };
    // Widened so the cross-multiplication cannot overflow at any counter
    // a provider could report, and stated as a product rather than a
    // ratio so no division rounds the threshold away.
    Ok(u128::from(last.prompt_tokens) * 100 >= u128::from(n) * u128::from(window))
}

/// Read the branch's last usage out of `worktree`'s transcript: the
/// highest-numbered `messages/NNN-<model-id>.json`, ignoring the
/// reserved `tool` origin and every `.md` delivery. `None` when the
/// branch has no model entry yet, or when its newest one carries no
/// `usage` sibling — the bare-array shape is lawful (§2.3).
pub(super) fn last(worktree: &Path) -> Result<Option<LastUsage>, Error> {
    let dir = worktree.join(MESSAGES_DIR);
    let mut newest: Option<(u32, String, std::path::PathBuf)> = None;
    let listing = match std::fs::read_dir(&dir) {
        Ok(rd) => rd,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(Error::Io(e)),
    };
    for entry in listing {
        let path = entry.map_err(Error::Io)?.path();
        let Some((seq, model)) = model_entry(&path) else {
            continue;
        };
        if newest.as_ref().is_none_or(|(s, _, _)| seq > *s) {
            newest = Some((seq, model, path));
        }
    }
    let Some((_, model, path)) = newest else {
        return Ok(None);
    };
    let bytes = std::fs::read(&path).map_err(Error::Io)?;
    Ok(report(&bytes, &model))
}

/// `(NNN, model-id)` of a `messages/NNN-<model-id>.json` path, or `None`
/// for anything else the directory holds: a `.md` delivery, the reserved
/// `tool` origin, a name with no counter prefix.
pub(super) fn model_entry(path: &Path) -> Option<(u32, String)> {
    if path.extension().and_then(|e| e.to_str()) != Some("json") {
        return None;
    }
    let stem = path.file_stem()?.to_string_lossy().into_owned();
    let (seq, origin) = stem.split_once('-')?;
    if origin == TOOL_ORIGIN || origin.is_empty() {
        return None;
    }
    Some((seq.parse::<u32>().ok()?, origin.to_string()))
}

/// The `usage` sibling of one entry's bytes, folded into a [`LastUsage`].
/// A bare block array, or an object with no `usage`, reports nothing.
pub(super) fn report(bytes: &[u8], model: &str) -> Option<LastUsage> {
    let usage = serde_json::from_slice::<Value>(bytes)
        .ok()?
        .get("usage")?
        .clone();
    let counter = |name: &str| usage.get(name).and_then(Value::as_u64).unwrap_or(0);
    Some(LastUsage {
        prompt_tokens: counter("input_tokens")
            + counter("cache_read_tokens")
            + counter("cache_write_tokens"),
        context_window: usage.get("context_window").and_then(Value::as_u64),
        model: model.to_string(),
    })
}

#[cfg(test)]
mod tests;
