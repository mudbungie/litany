//! `workflow` declines (ARCH §6 *The workflow mark*, §3.4): every
//! refusal precedes the mark — an unknown workspace, agent or lineage,
//! an unreadable or unparseable target head, an unwritable ref — and a
//! declined switch leaves no debris. Split from
//! [`workflowing`](super::workflowing) to hold the per-file line cap.

use super::{assert_prefixed, noop_editor, with_fx};
use crate::cmd::workflow;
use crate::template::{GitRunner, RealGit};
use crate::workspace::{self, fixture, workflow_mark};

/// [`super::workflowing`]'s happy-path driver, re-stated for the two
/// setup marks the failure cases below need in place first.
fn mark(ws: &std::path::Path, agent: &str) {
    let (r, ..) = with_fx("true", b"", &noop_editor, |fx| {
        workflow::run(
            workflow::Args {
                workspace: ws.to_path_buf(),
                agent: agent.to_string(),
                config: None,
                clear: false,
            },
            fx,
        )
    });
    r.unwrap();
}

#[test]
fn a_missing_lineage_declines_before_any_mark_is_written() {
    let (_h, ws) = fixture::workspace();
    fixture::spawn_root(&ws, "20260101-a1");
    let (r, ..) = with_fx("true", b"", &noop_editor, |fx| {
        workflow::run(
            workflow::Args {
                workspace: ws.clone(),
                agent: "20260101-a1".into(),
                config: Some("nosuch".into()),
                clear: false,
            },
            fx,
        )
    });
    assert_prefixed(r.unwrap_err(), "workflow");
    assert_eq!(
        workflow_mark::read(&ws, "20260101-a1", &RealGit::new()),
        None,
        "a declined pre-flight leaves no debris",
    );
}

#[test]
fn a_target_whose_workflow_does_not_parse_declines_before_the_mark() {
    // Validity precedes the mark: a standing mark must always name a
    // commit the resolver can read, so an unparseable `workflow.yaml`
    // at the lineage head refuses the switch outright.
    let (_h, ws) = fixture::workspace();
    fixture::spawn_root(&ws, "20260101-a1");
    fixture::amend_config(
        &ws,
        &[(
            "workflow.yaml",
            "events:\n  user_message: [not_an_action]\n",
        )],
    );
    let (r, ..) = with_fx("true", b"", &noop_editor, |fx| {
        workflow::run(
            workflow::Args {
                workspace: ws.clone(),
                agent: "20260101-a1".into(),
                config: None,
                clear: false,
            },
            fx,
        )
    });
    assert_prefixed(r.unwrap_err(), "workflow");
    assert_eq!(
        workflow_mark::read(&ws, "20260101-a1", &RealGit::new()),
        None,
    );
}

#[test]
fn a_target_failing_the_version_guard_declines_before_the_mark() {
    // §10: a config commit authored by a newer harness may carry shapes
    // the parsers cannot read — declined before interpreting any of them.
    let (_h, ws) = fixture::workspace();
    fixture::spawn_root(&ws, "20260101-a1");
    fixture::amend_config(&ws, &[("version", "not-a-version\n")]);
    let (r, ..) = with_fx("true", b"", &noop_editor, |fx| {
        workflow::run(
            workflow::Args {
                workspace: ws.clone(),
                agent: "20260101-a1".into(),
                config: None,
                clear: false,
            },
            fx,
        )
    });
    assert_prefixed(r.unwrap_err(), "workflow");
    assert_eq!(
        workflow_mark::read(&ws, "20260101-a1", &RealGit::new()),
        None,
    );
}

#[test]
fn an_unknown_agent_declines_with_the_uniform_failure() {
    let (_h, ws) = fixture::workspace();
    let (r, ..) = with_fx("true", b"", &noop_editor, |fx| {
        workflow::run(
            workflow::Args {
                workspace: ws.clone(),
                agent: "20260101-zz".into(),
                config: None,
                clear: false,
            },
            fx,
        )
    });
    assert_prefixed(r.unwrap_err(), "workflow");
}

#[test]
fn a_mark_that_cannot_be_written_surfaces_the_uniform_failure() {
    // The pre-flights are reads and pass against a read-only repo
    // (packed refs keep the lookups answering); the `update-ref` that
    // would write the mark then cannot take its lock, and the failure
    // arrives as the verb's own `litany workflow: …` line.
    use std::os::unix::fs::PermissionsExt;
    let (_h, ws) = fixture::workspace();
    fixture::spawn_root(&ws, "20260101-a1");
    RealGit::new()
        .run(&workspace::repo_git(&ws), &["pack-refs", "--all"])
        .unwrap();
    let repo = ws.join("repo.git/refs");
    std::fs::set_permissions(&repo, std::fs::Permissions::from_mode(0o555)).unwrap();
    let (r, ..) = with_fx("true", b"", &noop_editor, |fx| {
        workflow::run(
            workflow::Args {
                workspace: ws.clone(),
                agent: "20260101-a1".into(),
                config: None,
                clear: false,
            },
            fx,
        )
    });
    std::fs::set_permissions(&repo, std::fs::Permissions::from_mode(0o755)).unwrap();
    assert_prefixed(r.unwrap_err(), "workflow");
}

#[test]
fn a_clear_that_cannot_be_written_surfaces_the_uniform_failure() {
    // The pre-flights are reads and pass against a read-only repo; the
    // `update-ref -d` of the packed mark then cannot take its lock.
    use std::os::unix::fs::PermissionsExt;
    let (_h, ws) = fixture::workspace();
    fixture::spawn_root(&ws, "20260101-a1");
    mark(&ws, "20260101-a1");
    let git = RealGit::new();
    git.run(&workspace::repo_git(&ws), &["pack-refs", "--all"])
        .unwrap();
    let repo = ws.join("repo.git");
    std::fs::set_permissions(&repo, std::fs::Permissions::from_mode(0o555)).unwrap();
    let (r, ..) = with_fx("true", b"", &noop_editor, |fx| {
        workflow::run(
            workflow::Args {
                workspace: ws.clone(),
                agent: "20260101-a1".into(),
                config: None,
                clear: true,
            },
            fx,
        )
    });
    std::fs::set_permissions(&repo, std::fs::Permissions::from_mode(0o755)).unwrap();
    assert_prefixed(r.unwrap_err(), "workflow");
}

#[test]
fn a_lineage_head_that_does_not_resolve_declines_before_the_mark() {
    // A dangling loose ref: listed by the lineage enumeration, but not a
    // resolvable commit — the rev-parse refusal is the verb's own line.
    let (_h, ws) = fixture::workspace();
    fixture::spawn_root(&ws, "20260101-a1");
    let dir = ws.join("repo.git/refs/heads/config");
    std::fs::write(
        dir.join("broken"),
        "0123456789abcdef0123456789abcdef01234567\n",
    )
    .unwrap();
    let (r, ..) = with_fx("true", b"", &noop_editor, |fx| {
        workflow::run(
            workflow::Args {
                workspace: ws.clone(),
                agent: "20260101-a1".into(),
                config: Some("broken".into()),
                clear: false,
            },
            fx,
        )
    });
    assert_prefixed(r.unwrap_err(), "workflow");
    assert_eq!(
        workflow_mark::read(&ws, "20260101-a1", &RealGit::new()),
        None,
    );
}

#[test]
fn a_lineage_head_carrying_no_control_files_declines_naming_the_address() {
    // An orphan lineage whose head is an empty tree: the `version` read
    // fails, and the decline names `<commit>:version` — the control
    // file's one true address — before any mark is written.
    let (_h, ws) = fixture::workspace();
    fixture::spawn_root(&ws, "20260101-a1");
    let git = RealGit::new();
    let repo = workspace::repo_git(&ws);
    let tree = git
        .run_capture(&repo, &["mktree"])
        .unwrap()
        .trim()
        .to_string();
    let commit = git
        .run_capture(&repo, &["commit-tree", "-m", "empty", &tree])
        .unwrap()
        .trim()
        .to_string();
    git.run(&repo, &["update-ref", "refs/heads/config/empty", &commit])
        .unwrap();
    let (r, ..) = with_fx("true", b"", &noop_editor, |fx| {
        workflow::run(
            workflow::Args {
                workspace: ws.clone(),
                agent: "20260101-a1".into(),
                config: Some("empty".into()),
                clear: false,
            },
            fx,
        )
    });
    let err = r.unwrap_err().to_string();
    assert!(err.contains("version"), "{err}");
    assert_eq!(
        workflow_mark::read(&ws, "20260101-a1", &RealGit::new()),
        None,
    );
}
