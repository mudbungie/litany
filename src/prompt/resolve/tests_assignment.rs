//! The role **assignment's** optional knobs under follow-the-tip
//! (ARCH §4.3; split from [`super::tests`], which owns the
//! workflow-source seam, to hold the per-file line cap).
//!
//! `effort:` and `priority:` ride the same `providers.yaml` assignment
//! as the model pointer, so bl-403b's ruling carries them for free: an
//! edit to the governing lineage's head reaches a running agent at its
//! next resolution, with no re-fork and no per-agent act. Each test
//! walks the fact the whole way — the config commit, `WorkerConfig`,
//! and the borrowed `Resolved` the step loop actually reads.

use super::tests::Fx;
use super::{ConfigSource, resolve_worker};
use crate::workspace::fixture;

/// A `providers.yaml` whose `worker` row carries one extra `knob` line.
fn providers_with(knob: &str) -> String {
    format!(
        "roles:\n  worker:\n    provider: anthropic\n    model: claude-sonnet-5\n    {knob}\n  \
         compactor:\n    provider: anthropic\n    model: claude-haiku-4-5\n"
    )
}

#[test]
fn the_role_effort_follows_the_tip_to_the_resolved_shape() {
    let (_h, ws) = fixture::workspace();
    fixture::spawn_root(&ws, "20260101-r1");
    let fx = Fx::new();
    let before = resolve_worker(&ws, ConfigSource::Agent("20260101-r1"), &fx.deps()).unwrap();
    assert_eq!(before.effort, None, "the shipped template requests none");
    fixture::amend_config(
        &ws,
        &[("providers.yaml", &providers_with("effort: medium"))],
    );
    let cfg = resolve_worker(&ws, ConfigSource::Agent("20260101-r1"), &fx.deps()).unwrap();
    assert_eq!(cfg.effort, Some(crate::config::Effort::Medium));
    assert_eq!(
        cfg.as_resolved().effort,
        Some(crate::config::Effort::Medium)
    );
}

#[test]
fn the_role_priority_follows_the_tip_to_the_resolved_shape() {
    // §4.3 `priority:` × follow-the-tip: checking the box on the
    // lineage's head moves every following agent onto the provider's
    // priority lane at its next step boundary — that IS the switch, and
    // unchecking it is the same act in reverse.
    let (_h, ws) = fixture::workspace();
    fixture::spawn_root(&ws, "20260101-r1");
    let fx = Fx::new();
    let before = resolve_worker(&ws, ConfigSource::Agent("20260101-r1"), &fx.deps()).unwrap();
    assert_eq!(before.priority, None, "the shipped template asks no lane");
    fixture::amend_config(
        &ws,
        &[("providers.yaml", &providers_with("priority: true"))],
    );
    let cfg = resolve_worker(&ws, ConfigSource::Agent("20260101-r1"), &fx.deps()).unwrap();
    assert_eq!(cfg.priority, Some(true));
    assert_eq!(cfg.as_resolved().priority, Some(true));
    // And back off the lane, on the same one act.
    fixture::amend_config(
        &ws,
        &[("providers.yaml", &providers_with("priority: false"))],
    );
    let after = resolve_worker(&ws, ConfigSource::Agent("20260101-r1"), &fx.deps()).unwrap();
    assert_eq!(after.priority, Some(false));
}
