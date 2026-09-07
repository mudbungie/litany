//! The `budgets:` block of `workflow.yaml` (ARCH §6 "Budgets (v0.7)"):
//! what parses, what an omitted axis means, and what the shipped
//! template actually declares. Split out of [`super::workflow_yaml`] at
//! the per-file line cap, on the same seam
//! [`super::workflow_compaction`] took: one block, one file.

use crate::config::error::LoadError;
use crate::config::workflow::{Budgets, Workflow};
use std::path::Path;

/// Parse `raw` as a `workflow.yaml`, labelled as the shipped file is.
fn parse(raw: &str) -> Result<Workflow, LoadError> {
    Workflow::parse(raw, Path::new("<commit>:workflow.yaml"))
}

#[test]
fn parses_explicit_budgets_block() {
    // ARCH §6 budgets example: all three limits declared.
    let w = parse("events: {}\nbudgets:\n  max_total_tokens: 2000000\n  max_wall_seconds: 3600\n  max_depth: 4\n").unwrap();
    assert_eq!(w.budgets.max_total_tokens, Some(2_000_000));
    assert_eq!(w.budgets.max_wall_seconds, Some(3600));
    assert_eq!(w.budgets.max_depth, Some(4));
}

#[test]
fn omitted_budgets_block_is_all_unbounded() {
    // No `budgets:` → Budgets::default (every axis None = unbounded).
    let w = parse("events:\n  user_message:\n    - land_compaction\n").unwrap();
    assert_eq!(w.budgets, Budgets::default());
    assert!(w.budgets.max_total_tokens.is_none());
    assert!(w.budgets.max_wall_seconds.is_none());
    assert!(w.budgets.max_depth.is_none());
}

#[test]
fn the_shipped_template_bounds_depth_alone() {
    // ARCH §6 "No SPEND ships bounded; depth does" (2026-08-16 operator
    // ruling, narrowed by bl-c701). These are the exact bytes `litany
    // new` writes into the first config commit (pinned by
    // template/tests_override.rs), so this is the workspace's own state
    // and not just the parser's.
    //
    // Both directions matter. A `max_depth` that silently disappeared
    // would take away the tree's ONLY prohibition on growth — the axis
    // whose absence produces a runaway rather than merely an expensive
    // tree. A spend ceiling that silently appeared would reinstate the
    // whole-tree cliff the ruling took out: a root and its whole
    // descent share one allowance, so such a ceiling ends conversations
    // that are working correctly, and raising its number only moves the
    // cliff. The number's reasoning lives beside it in
    // `template/workflow.yaml`; this test refuses a silent move, and
    // the gate's own off-by-one is
    // `prompt/tests/budget_depth_boundary.rs`'s.
    let raw = crate::template::TEMPLATE
        .get_file("workflow.yaml")
        .expect("the template ships a workflow.yaml")
        .contents_utf8()
        .expect("utf8");
    let w = parse(raw).unwrap();
    assert_eq!(
        w.budgets.max_depth,
        Some(5),
        "a root plus five levels of delegation; see workflow.yaml before moving it"
    );
    assert_eq!(
        w.budgets.max_total_tokens, None,
        "a whole-tree token ceiling is the operator's to declare (2026-08-16)"
    );
    assert_eq!(
        w.budgets.max_wall_seconds, None,
        "a whole-tree wall ceiling is the operator's to declare (2026-08-16)"
    );
}

#[test]
fn partial_budgets_leaves_the_other_axes_unbounded() {
    // A single declared limit; the rest stay unbounded (§6).
    let w = parse("events: {}\nbudgets:\n  max_total_tokens: 500\n").unwrap();
    assert_eq!(w.budgets.max_total_tokens, Some(500));
    assert!(w.budgets.max_wall_seconds.is_none());
    assert!(w.budgets.max_depth.is_none());
}
