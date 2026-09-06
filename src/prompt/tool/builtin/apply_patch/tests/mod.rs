//! Tests for the `apply_patch` built-in: grammar ([`parse`]), the
//! matching ladder ([`seek`]), atomic application ([`apply`]), and the
//! stdio entry ([`run`]). Split per module to keep every file under the
//! repo's per-file line cap.

use super::parse::parse;

mod applier;
mod applier_io;
mod declines;
mod grammar;
mod ladder;
mod stdio;
mod symlink;

/// Wrap a body in the envelope markers.
pub(super) fn envelope(body: &str) -> String {
    format!("*** Begin Patch\n{body}\n*** End Patch")
}

/// Parse an envelope-wrapped body, asserting it parses.
pub(super) fn parsed(body: &str) -> super::parse::Patch {
    parse(&envelope(body)).expect("body parses")
}
