//! Tests for the inbox substrate (ARCH §2.11), split by axis so each
//! file stays under the repo's per-file line cap.
//!
//! - [`lock`]: `flock` acquire / exclusion / release-on-drop and the
//!   errno interpretation.
//! - [`deposit`]: create-only atomicity, frontmatter, and sender
//!   sequence derivation.
//! - [`probe`]: the deposit-starts-a-driver decision, sender
//!   resolution, and the `cli_message` / `cli_run` orchestration.
//! - [`launcher`]: what a launch actually does — the detached spawn,
//!   its `driver.log` stderr sink, and the declines around both.

mod deposit;
mod launcher;
mod lock;
mod probe;
mod result;
