//! The workflow-source seam's declines (split from [`super::tests`] to
//! hold the per-file line cap): a marked commit the resolver cannot
//! read — version guard, closed-vocabulary parse, a commit with no
//! `workflow.yaml` at all — declines loudly rather than being misread
//! or silently fallen back from.

use super::tests::{Fx, SWITCHED_WORKFLOW, head};
use super::{ConfigSource, resolve_worker};
use crate::template::{GitRunner, RealGit};
use crate::workspace::{self, fixture, workflow_mark};
use std::path::Path;

#[test]
fn a_marked_commit_failing_the_version_guard_declines_resolution() {
    // §10 discipline holds for the marked commit too: its workflow may
    // carry shapes this harness cannot read, so the guard runs before
    // the parse — declined loudly, not misread.
    let (_h, ws) = fixture::workspace();
    fixture::spawn_root(&ws, "20260101-r1");
    fixture::amend_config(
        &ws,
        &[
            ("version", "not-a-version\n"),
            ("workflow.yaml", SWITCHED_WORKFLOW),
        ],
    );
    let fx = Fx::new();
    workflow_mark::write(&ws, "20260101-r1", &head(&ws), &fx.git).unwrap();
    let err = match resolve_worker(&ws, ConfigSource::Agent("20260101-r1"), &fx.deps()) {
        Err(err) => err,
        Ok(_) => panic!("a marked commit with a bad version must decline"),
    };
    assert!(err.to_string().contains("version"), "{err}");
}

#[test]
fn a_marked_commit_whose_workflow_does_not_parse_declines_resolution() {
    // The verb pre-flights this, but a mark is a ref anyone can write:
    // resolution still declines loudly rather than stepping on a policy
    // it cannot read.
    let (_h, ws) = fixture::workspace();
    fixture::spawn_root(&ws, "20260101-r1");
    fixture::amend_config(
        &ws,
        &[(
            "workflow.yaml",
            "events:\n  user_message: [not_an_action]\n",
        )],
    );
    let fx = Fx::new();
    workflow_mark::write(&ws, "20260101-r1", &head(&ws), &fx.git).unwrap();
    assert!(resolve_worker(&ws, ConfigSource::Agent("20260101-r1"), &fx.deps()).is_err());
}

#[test]
fn a_mark_at_a_commit_with_no_workflow_declines_as_a_control_read() {
    // A mark aimed at a commit that carries no `workflow.yaml` at all —
    // an agent's own tip, say — is a defective mark; the control read
    // names the missing address instead of silently falling back.
    let (_h, ws) = fixture::workspace();
    fixture::spawn_root(&ws, "20260101-r1");
    let fx = Fx::new();
    let orphan = orphan_commit(&ws, &fx.git);
    workflow_mark::write(&ws, "20260101-r1", &orphan, &fx.git).unwrap();
    assert!(resolve_worker(&ws, ConfigSource::Agent("20260101-r1"), &fx.deps()).is_err());
}

/// An empty orphan commit in the workspace repo — a commit-ish carrying
/// none of the control files.
fn orphan_commit(ws: &Path, git: &RealGit) -> String {
    let repo = workspace::repo_git(ws);
    let tree = git
        .run_capture(&repo, &["mktree"])
        .unwrap()
        .trim()
        .to_string();
    git.run_capture(&repo, &["commit-tree", "-m", "empty", &tree])
        .unwrap()
        .trim()
        .to_string()
}
