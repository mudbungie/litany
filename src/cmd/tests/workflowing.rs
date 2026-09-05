//! `workflow` (ARCH §6 *The workflow mark*, §3.4): the pre-flights, the
//! mark, and the clear — the switch itself lands at resolution
//! (`prompt::resolve::workflow_source`), which is pinned there.

use super::{noop_editor, with_fx};
use crate::cmd::{Outcome, workflow};
use crate::template::{GitRunner, RealGit};
use crate::workspace::{self, fixture, workflow_mark};

/// Drive the verb, returning its `Outcome`. A write's confirmation is
/// an `eprintln!` on the process's own stderr (§3.4), so what a test
/// reads back after one is the mark itself; a read's whole answer is
/// the `Outcome::Line` product.
fn call(
    ws: &std::path::Path,
    agent: &str,
    config: Option<&str>,
    clear: bool,
) -> crate::cmd::Outcome {
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
    assert!(out.is_empty(), "the product is the Outcome, not a print");
    r.unwrap()
}

/// A write: product-less on stdout (§3.4).
fn run(ws: &std::path::Path, agent: &str, config: Option<&str>, clear: bool) {
    assert!(
        matches!(call(ws, agent, config, clear), Outcome::Quiet),
        "a write is product-less (§3.4)",
    );
}

/// The read (bl-5c02): the one line the bare invocation answers with.
fn read(ws: &std::path::Path, agent: &str) -> String {
    match call(ws, agent, None, false) {
        Outcome::Line(line) => line,
        other => panic!("the read answers one line, got {other:?}"),
    }
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
fn an_unnamed_config_reads_and_writes_nothing() {
    // The default that used to stand here (`--config default`) is gone
    // (bl-5c02): the gesture that reads most like an inspection must
    // not silently pin an agent, so bare is the read and a write names
    // its target.
    let (_h, ws) = fixture::workspace();
    fixture::spawn_root(&ws, "20260101-a1");
    let line = read(&ws, "20260101-a1");
    assert!(line.starts_with("20260101-a1 runs "), "{line}");
    assert!(line.contains(&head(&ws, "default")[..12]), "{line}");
    assert!(line.contains("(config/default)"), "{line}");
    assert!(
        line.contains("followed from its governing lineage"),
        "{line}"
    );
    assert_eq!(
        workflow_mark::read(&ws, "20260101-a1", &RealGit::new()),
        None,
        "a read writes no ref",
    );
}

#[test]
fn the_read_names_the_mark_and_the_agent_holding_it() {
    let (_h, ws) = fixture::workspace();
    fixture::spawn_root(&ws, "20260101-a1");
    let pinned = head(&ws, "default");
    fixture::amend_config(&ws, &[("workflow.yaml", "events: {}\n")]);
    run(&ws, "20260101-a1", Some("default"), false);
    let line = read(&ws, "20260101-a1");
    assert!(line.contains(&head(&ws, "default")[..12]), "{line}");
    assert!(line.contains("marked on [20260101-a1]"), "{line}");
    assert!(!line.contains("ancestor"), "{line}");
    assert_ne!(pinned, head(&ws, "default"), "the lineage really advanced");
}

#[test]
fn the_read_names_the_ancestor_whose_mark_a_child_inherits() {
    // The half an operator cannot derive from the agent they asked
    // about: marking a root switches its whole tree (§6).
    let (_h, ws) = fixture::workspace();
    fixture::spawn_root(&ws, "20260101-a1");
    fixture::spawn_agent(&ws, "20260101-a1-20260102-c1", "agents/20260101-a1");
    run(&ws, "20260101-a1", Some("default"), false);
    let line = read(&ws, "20260101-a1-20260102-c1");
    assert!(line.contains("marked on ancestor [20260101-a1]"), "{line}");
}

#[test]
fn a_mark_on_a_commit_no_lineage_stands_on_renders_the_absence() {
    // Ordinary, not a defect: the mark pins an older commit on purpose
    // and the lineage has advanced past it, so no `config/*` ref points
    // there. The sha is the unambiguous half.
    let (_h, ws) = fixture::workspace();
    fixture::spawn_root(&ws, "20260101-a1");
    let pinned = head(&ws, "default");
    run(&ws, "20260101-a1", Some("default"), false);
    fixture::amend_config(&ws, &[("workflow.yaml", "events: {}\n")]);
    let line = read(&ws, "20260101-a1");
    assert!(line.contains(&pinned[..12]), "{line}");
    assert!(!line.contains("(config/"), "{line}");
}

#[test]
fn clear_deletes_the_mark_and_deletes_config_not_code() {
    let (_h, ws) = fixture::workspace();
    fixture::spawn_root(&ws, "20260101-a1");
    run(&ws, "20260101-a1", Some("default"), false);
    run(&ws, "20260101-a1", None, true);
    assert_eq!(
        workflow_mark::read(&ws, "20260101-a1", &RealGit::new()),
        None,
        "cleared: the followed config's workflow governs again",
    );
    assert!(
        read(&ws, "20260101-a1").contains("followed from its governing lineage"),
        "and the read says so",
    );
}

#[test]
fn the_read_says_so_when_diverged_lineages_hold_the_agent() {
    // Control is held at the fork commit while two lineages reach the
    // agent (§2.2), and the read reports the hold rather than calling
    // the fork commit "the governing lineage" — the notice the resolver
    // prints at every step, available on demand instead of by waiting
    // for one.
    let (_h, ws) = fixture::workspace();
    fixture::spawn_root(&ws, "20260101-a1");
    let fork = head(&ws, "default");
    RealGit::new()
        .run(
            &workspace::repo_git(&ws),
            &["update-ref", "refs/heads/config/variant", &fork],
        )
        .unwrap();
    fixture::amend_config(&ws, &[("souls/worker.md", "a newer soul\n")]);
    let line = read(&ws, "20260101-a1");
    assert!(line.contains(&fork[..12]), "{line}");
    assert!(
        line.contains("followed from its fork commit — 2 diverged config lineages"),
        "{line}"
    );
}
