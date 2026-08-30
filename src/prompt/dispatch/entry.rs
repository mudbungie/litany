//! The committed transcript entry's on-disk shape — the one home for it
//! (ARCH §2.3 *Origins and wire framing*, *The transcript writer*).
//!
//! A `messages/NNN-<origin>.json` entry is an **API-shaped message
//! object**: the canonical [`Content`] blocks under `content`, with the
//! provider's token `usage` report as its sibling when the provider
//! reported one:
//!
//! ```json
//! {"content":[{"type":"text","text":"hi"}],"usage":{"input_tokens":5,"output_tokens":3}}
//! ```
//!
//! **A bare array of blocks is equally lawful** — the shape every entry
//! carried before usage rode along, and the shape a tool entry still
//! carries. So the reader answers one question over both — *where do the
//! blocks live* ([`blocks`]) — and absence of `usage` is the general path
//! with empty inputs, never an error (`docs/PRINCIPLES.md`).
//!
//! **Usage is the provider's report, never litany's arithmetic.** A
//! provider may state one report in installments — Anthropic's
//! `message_start` carries the input side, its terminal `message_delta`
//! the output side — so [`UsageReport`] records each counter *as the
//! provider last reported it* and omits every counter the provider never
//! reported (a `0` would be a lie, brazen's zero-vs-unknown rule). It
//! adds nothing, scales nothing, estimates nothing: the sealed object is
//! what the same call would have returned unstreamed. The fold is over
//! the serialized counter names rather than named fields, so a counter
//! brazen adds under `v=1` (its [`Usage`] is `#[non_exhaustive]`) rides
//! through with no edit here.
//!
//! Spend metering is a *different* fact and keeps its own home: §6/§8
//! bill every attempt segment of the diagnostic `response.json`,
//! including the discarded ones (`crate::prompt::budget`). This entry
//! records only what the committed output itself cost — the authoritative
//! segment's report (§4.4 segment authority).

use brazen::{Content, Usage};
use serde::Deserialize;
use serde_json::{Map, Value};

/// The provider's usage report for the entry under construction, folded
/// per counter as `usage` events arrive. Empty — the default — is "the
/// provider reported nothing", which seals no `usage` key at all.
#[derive(Default)]
pub(super) struct UsageReport(Map<String, Value>);

impl UsageReport {
    /// Fold one `usage` event's counters in: a counter the event reports
    /// supersedes any earlier value, a counter it leaves unreported
    /// (`null`) leaves the earlier one standing.
    pub(super) fn fold(&mut self, usage: &Usage) {
        let reported = serde_json::to_value(usage).expect("Usage serializes");
        for (counter, value) in reported.as_object().into_iter().flatten() {
            if !value.is_null() {
                self.0.insert(counter.clone(), value.clone());
            }
        }
    }
}

/// The bytes that open an entry under construction: the object up to
/// its first block. The array stays open until [`close`].
pub(super) fn open() -> &'static [u8] {
    br#"{"content":["#
}

/// The bytes that close a sealed entry: the `content` array, then the
/// provider's `usage` sibling iff any counter was reported.
pub(super) fn close(usage: &UsageReport) -> Vec<u8> {
    let mut out = b"]".to_vec();
    if !usage.0.is_empty() {
        out.extend_from_slice(br#","usage":"#);
        out.extend_from_slice(
            &serde_json::to_vec(&usage.0).expect("a usage counter map serializes"),
        );
    }
    out.push(b'}');
    out
}

/// **Where a committed entry's content blocks live** — the one answer,
/// used by every reader (assembly §5, the writer's read-back at commit,
/// the unsettled-tail scan). Harness-written (the staging seal /
/// `commit_tool`, §2.3), so neither lawful shape can fail to parse and a
/// failure is a programmer error, not a reachable state.
pub(super) fn blocks(bytes: &[u8]) -> Vec<Content> {
    let payload: Payload = serde_json::from_slice(bytes)
        .expect("transcript entry is a canonical Content array or a `content` object");
    match payload {
        Payload::Object { content } => content,
        Payload::Bare(content) => content,
    }
}

/// The two lawful entry shapes (above). Tried in order, so the object
/// shape claims an object and the bare array an array; `usage` and any other
/// sibling is ignored here — no litany reader consumes it, and the
/// committed bytes are its home for the readers that do.
#[derive(Deserialize)]
#[serde(untagged)]
enum Payload {
    Object { content: Vec<Content> },
    Bare(Vec<Content>),
}

#[cfg(test)]
mod tests;
