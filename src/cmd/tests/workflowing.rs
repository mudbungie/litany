//! `workflow` (ARCH §6 *The workflow mark*, §3.4): the pre-flights, the
//! mark, and the clear — the switch itself lands at resolution
//! (`prompt::resolve::workflow_source`), which is pinned there.

use super::{noop_editor, with_fx};
use crate::cmd::{Outcome, workflow};
use crate::template::{GitRunner, RealGit};
use crate::workspace::{self, fixture, workflow_mark};

/// The verb's confirmation is an `eprintln!` on the process's own stderr
/// (§3.4), so what a test reads back is the mark itself: the one thing
/// the verb writes.
fn run(ws: &std::path::Path, agent: &str, config: Option<&str>, clear: bool) {
    let (r, out, _err) = with_fx("true", b"", &noop_editor, |fx| {
        workflow::run(
            workflow::Args {
                workspace: ws.to_path_buf(),
                agent: agent.to_string(),
                config: config.map(str::to_string),
                clear,
            },
            fx,
        )
    });
    assert!(matches!(r.unwrap(), Outcome::Quiet), "product-less (§3.4)");
    assert!(out.is_empty(), "no stdout product (§3.4)");
}

fn head(ws: &std::path::Path, lineage: &str) -> String {
    RealGit::new()
        .run_capture(
            &workspace::repo_git(ws),
            &["rev-parse", &workspace::config_ref(lineage)],
        )
        .unwrap()
        .trim()
        .to_string()
}

#[test]
fn a_switch_writes_the_standing_mark_at_the_named_lineages_head() {
    let (_h, ws) = fixture::workspace();
    fixture::spawn_root(&ws, "20260101-a1");
    fixture::amend_config(
        &ws,
        &[(
            "workflow.yaml",
            "events: {}\nretry:\n  max_attempts: 2\n  backoff: exponential\n",
        )],
    );
    let want = head(&ws, "default");
    run(&ws, "20260101-a1", Some("default"), false);
    assert_eq!(
        workflow_mark::read(&ws, "20260101-a1", &RealGit::new()),
        Some(want),
        "the mark is the verb's whole effect",
    );
}

#[test]
fn an_unnamed_config_marks_the_default_lineage() {
    // The general path with empty inputs — the same reading every other
    // config-naming verb gives an unnamed config.
    let (_h, ws) = fixture::workspace();
    fixture::spawn_root(&ws, "20260101-a1");
    run(&ws, "20260101-a1", None, false);
    assert_eq!(
        workflow_mark::read(&ws, "20260101-a1", &RealGit::new()),
        Some(head(&ws, "default")),
    );
}

#[test]
fn clear_deletes_the_mark_and_deletes_config_not_code() {
    let (_h, ws) = fixture::workspace();
    fixture::spawn_root(&ws, "20260101-a1");
    run(&ws, "20260101-a1", None, false);
    run(&ws, "20260101-a1", None, true);
    assert_eq!(
        workflow_mark::read(&ws, "20260101-a1", &RealGit::new()),
        None,
        "cleared: the followed config's workflow governs again",
    );
}
