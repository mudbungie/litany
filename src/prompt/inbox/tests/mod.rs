//! Tests for the inbox substrate (ARCH §2.11), split by axis so each
//! file stays under the repo's per-file line cap. Since bl-6a7c the
//! source is split on the same axes — [`super::launch`] under
//! [`launcher`] and [`probe`], [`super::cli`] under [`probe`]'s
//! orchestration beats — which is what made the seam a real one rather
//! than a line count.
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
