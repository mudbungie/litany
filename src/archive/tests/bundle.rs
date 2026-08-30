//! `bundle` cases (ARCH §9.2): subtree enumeration, slice copying, the
//! error arms, and the §10 layout guard. The shared stub and fixtures
//! live in the parent [`super`] module.

use super::super::{ArchiveError, bundle};
use super::{AGENT, REFS, StubGit, tmp, write, ws_tmp};
use crate::workspace;
use std::fs;

#[test]
fn bundle_writes_bundle_and_matching_slices() {
    let ws = ws_tmp();
    // A matching agent step dir with a file, plus an unrelated sibling.
    write(&ws.path().join("steps/20260101-p1/001/meta.json"), "{}");
    write(&ws.path().join("steps/20260101-other/001/meta.json"), "{}");
    // No inbox dir at all — exercises the missing-slice no-op.
    let out = tmp();
    let git = StubGit::new(REFS);

    bundle(ws.path(), AGENT, out.path(), &git).unwrap();

    // The bundle-create ref list is the enumerated subtree plus the
    // governing lineage (§9.2 — the replayed workspace needs a `config/*`
    // ref to take the merge-base against).
    let runs = git.runs.borrow();
    assert_eq!(runs[0][0], "bundle");
    assert_eq!(runs[0][1], "create");
    assert!(runs[0].contains(&"agents/20260101-p1".to_owned()));
    assert!(runs[0].contains(&"agents/20260101-p1-20260102-c1".to_owned()));
    assert!(runs[0].contains(&"refs/heads/config/default".to_owned()));
    assert!(runs[0].contains(&"refs/heads/config/strict".to_owned()));
    // The matching slice copied; the unrelated sibling did not.
    assert!(out.path().join("steps/20260101-p1/001/meta.json").exists());
    assert!(!out.path().join("steps/20260101-other").exists());
    assert!(!out.path().join("inbox").exists());
}

#[test]
fn bundle_rejects_unknown_agent() {
    let ws = ws_tmp();
    let out = tmp();
    let git = StubGit::new(""); // no branches match
    let err = bundle(ws.path(), AGENT, out.path(), &git).unwrap_err();
    assert!(
        matches!(err, ArchiveError::UnknownAgent(ref a) if a == AGENT),
        "{err:?}"
    );
}

#[test]
fn bundle_surfaces_branch_list_failure() {
    let ws = ws_tmp();
    let out = tmp();
    let git = StubGit::new(REFS).fail_capture();
    let err = bundle(ws.path(), AGENT, out.path(), &git).unwrap_err();
    assert!(
        matches!(
            err,
            ArchiveError::Git {
                op: "branch --list",
                ..
            }
        ),
        "{err:?}"
    );
}

#[test]
fn bundle_surfaces_bundle_create_failure() {
    let ws = ws_tmp();
    let out = tmp();
    let git = StubGit::new(REFS).fail_run_at(0);
    let err = bundle(ws.path(), AGENT, out.path(), &git).unwrap_err();
    assert!(
        matches!(
            err,
            ArchiveError::Git {
                op: "bundle create",
                ..
            }
        ),
        "{err:?}"
    );
}

#[test]
fn bundle_carries_only_config_lineages_that_reach_the_subtree() {
    // Both config branches are orphans to this agent (no merge-base), so
    // neither governs it and neither rides — the bundle is the subtree
    // alone, exactly as before the lineage landed.
    let ws = ws_tmp();
    let out = tmp();
    let git = StubGit::new(REFS).no_lineage();

    bundle(ws.path(), AGENT, out.path(), &git).unwrap();

    let runs = git.runs.borrow();
    assert!(!runs[0].iter().any(|a| a.starts_with("refs/heads/config/")));
}

#[test]
fn bundle_surfaces_a_lineage_enumeration_failure() {
    let ws = ws_tmp();
    let out = tmp();
    let git = StubGit::new(REFS).fail_lineage();
    let err = bundle(ws.path(), AGENT, out.path(), &git).unwrap_err();
    assert!(
        matches!(
            err,
            ArchiveError::Git {
                op: "config lineage",
                ..
            }
        ),
        "{err:?}"
    );
}

#[test]
fn bundle_refuses_the_retired_layout_with_an_actionable_error() {
    // A `root/` primary worktree (the retired per-conversation layout)
    // and no `repo.git`: the §10 guard declines before any git op,
    // naming what was found (`bundle` is a verb).
    let ws = tmp();
    fs::create_dir_all(ws.path().join("root/.git")).unwrap();
    let out = tmp();
    let git = StubGit::new(REFS);
    let err = bundle(ws.path(), AGENT, out.path(), &git).unwrap_err();
    let msg = err.to_string();
    assert!(
        matches!(
            err,
            ArchiveError::Layout(workspace::LayoutError::OldLayout(_))
        ),
        "{err:?}"
    );
    assert!(msg.contains("retired per-conversation layout"), "{msg}");
    assert!(msg.contains("litany new"), "{msg}");
    // No git op ran — the guard short-circuits before enumeration.
    assert!(
        git.runs.borrow().is_empty(),
        "guard must precede any git op"
    );
}

#[test]
fn bundle_refuses_a_non_workspace() {
    let ws = tmp(); // bare dir: no repo.git, no old-layout signature
    let out = tmp();
    let git = StubGit::new(REFS);
    let err = bundle(ws.path(), AGENT, out.path(), &git).unwrap_err();
    assert!(
        matches!(
            err,
            ArchiveError::Layout(workspace::LayoutError::NotAWorkspace(_))
        ),
        "{err:?}"
    );
    assert!(err.to_string().contains("litany new"), "{err}");
}
