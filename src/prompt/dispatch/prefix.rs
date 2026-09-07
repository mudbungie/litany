//! The **hold** on the declared surface (ARCH §5.5 *prefix edits ride a
//! paid miss*; yog bl-b6f9, litany bl-b902).
//!
//! Provider prompt caching is keyed on the request's leading bytes in
//! cache order — tools, then system, then messages — so any byte that
//! moves ahead of the transcript tail re-bills the whole context behind
//! it. Between two steps the composed `tools` array can move for a
//! reason nobody chose to pay for: a host retired a tool it had injected
//! (yog's `clients unload`), a followed config commit revoked a grant.
//! Each is a **subtraction** — the model loses nothing it needs by not
//! hearing about it yet, since a call into a retired name is refused in
//! band exactly as today — and each, landed on its own, costs a full
//! rebuild.
//!
//! **The invariant: a prefix edit is applied only at a boundary that
//! already invalidates the prefix.** A subtraction from the declared
//! surface is *held*: the request keeps sending the array the previous
//! step sent, byte for byte, until some other part of the prefix moves
//! anyway — the model or the system slot changed (a config edit, a
//! retarget), or the first wire message did (a compaction landing, the
//! head recomposed) — and then the current composition goes out whole,
//! the subtraction riding a miss that was being paid regardless. An
//! **addition** is never held: a tool the model must be able to reach
//! now is a miss the act itself chose, and it carries every pending
//! subtraction with it, since the array is re-sent whole.
//!
//! **The queue's home is the previous step's own record.** The last
//! request sent is on disk at `steps/<agent>/<NNN>/request.json` —
//! written before the call, recorded rather than re-derived, the same
//! discipline `meta.json` keeps — so the held array is read back from
//! there and nowhere else: no second tree path, no boundary flag on the
//! executor, no list of boundary kinds to keep in step with the acts
//! that make them. Whether the miss is inevitable is answered by
//! comparing the two records, which is the only place the question can
//! be answered without re-deriving what the adapter was actually handed.
//! The first step of a branch, or one whose predecessor left no request,
//! sends its own composition — the general path with empty inputs.
//!
//! What this holds is the **tools array alone**. Head material rides the
//! first wire message and is a function of the tree; a subtraction there
//! (an agent's `rm`, §5.4) is priced from its position as before. The
//! hold compares serialized forms — `request.json` is the serialized
//! request — so a round trip through brazen's decoder is never on the
//! comparison path.

use crate::prompt::Error;
use crate::prompt::step::{REQUEST_FILE, step_dir_rel};
use brazen::{CanonicalRequest, Tool};
use serde_json::Value;
use std::path::Path;

/// The request `agent`'s step before `seq` sent, as recorded — `None`
/// for a first step or a predecessor with no request on disk.
pub(super) fn previous(workspace: &Path, agent: &str, seq: u32) -> Result<Option<Value>, Error> {
    let Some(prev) = seq.checked_sub(1).filter(|s| *s > 0) else {
        return Ok(None);
    };
    let path = workspace.join(step_dir_rel(agent, prev)).join(REQUEST_FILE);
    match std::fs::read(&path) {
        Ok(bytes) => Ok(Some(
            serde_json::from_slice(&bytes).map_err(Error::AdapterJson)?,
        )),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(Error::Io(e)),
    }
}

/// Apply the hold (module docs): `current` with its `tools` replaced by
/// `previous`'s when the prefix ahead of the tail is otherwise intact and
/// the current array is a strict subsequence of the previous one. A
/// record whose held array does not read back as tools is a fault of the
/// record, surfaced rather than sent around.
pub(super) fn hold(
    previous: Option<&Value>,
    mut current: CanonicalRequest,
) -> Result<CanonicalRequest, Error> {
    let Some(prev) = previous else {
        return Ok(current);
    };
    let now = serde_json::to_value(&current).expect("CanonicalRequest is always serializable");
    let intact = prev["model"] == now["model"]
        && prev["system"] == now["system"]
        && head_intact(&prev["messages"], &now["messages"]);
    let prev_tools = prev["tools"].as_array().cloned().unwrap_or_default();
    let now_tools = now["tools"].as_array().cloned().unwrap_or_default();
    if intact && now_tools.len() < prev_tools.len() && is_subsequence(&now_tools, &prev_tools) {
        current.tools = serde_json::from_value::<Vec<Tool>>(Value::Array(prev_tools))
            .map_err(Error::AdapterJson)?;
    }
    Ok(current)
}

/// The first wire message of `prev` is a prefix of the first of `now`:
/// the head blocks and whatever first entry grouped onto them (§2.3)
/// are where they were, and only the tail has moved.
fn head_intact(prev: &Value, now: &Value) -> bool {
    let first = |m: &Value| m[0]["content"].as_array().cloned().unwrap_or_default();
    now[0].is_null() && prev[0].is_null() || first(now).starts_with(&first(prev))
}

/// `sub` is `of` with zero or more entries removed, order kept.
fn is_subsequence(sub: &[Value], of: &[Value]) -> bool {
    let mut rest = of.iter();
    sub.iter().all(|t| rest.any(|o| o == t))
}

#[cfg(test)]
mod tests;
