//! **A step may not advance its own conversation's lineage** (bl-d273,
//! round-1 triage ruling 3; origin yog bl-baed).
//!
//! The escalation these beats close, measured on a live drive: an agent
//! with the shipped `bash` grant found `litany` on the world's PATH,
//! wrote a script as `$EDITOR` and ran `litany config <workspace>` —
//! advancing the very lineage that holds its own soul, grant, model and
//! facts. The two verbs that advance a lineage refuse under the §3.3
//! `LITANY_TOOL_ID` marker; everything a step may still do is asserted
//! here beside them, because a guard that also refused the reads would
//! be a different (and wrong) rule.

use super::{noop_editor, with_fx_id, with_litany_home, writing_editor};
use crate::cmd::{Outcome, config, proposal};
use crate::template::{GitRunner, RealGit};
use crate::workspace::{config_ref, fixture, proposal::proposal_ref, repo_git};
use std::path::{Path, PathBuf};
use tempfile::TempDir;

/// The `tool_use.id` shape the executor exports (ARCH §3.3).
const IN_A_STEP: Option<&str> = Some("toolu_01A9bK");

fn tip(ws: &Path) -> String {
    RealGit::new()
        .run_capture(&repo_git(ws), &["rev-parse", &config_ref("default")])
        .unwrap()
}

fn config_args(ws: &Path) -> config::Args {
    config::Args {
        workspace: ws.to_path_buf(),
        name: None,
        from: None,
        orphan: false,
    }
}

fn proposal_args(ws: &Path, id: Option<&str>, accept: bool, reject: bool) -> proposal::Args {
    proposal::Args {
        workspace: ws.to_path_buf(),
        id: id.map(str::to_owned),
        accept,
        reject,
    }
}

/// A workspace with one staged proposal on the default lineage — the
/// same shape [`super::proposing`] stages, spelled here so the two files
/// share no fixture state.
fn workspace_with_a_proposal(id: &str) -> (TempDir, PathBuf) {
    let (h, ws) = fixture::workspace();
    let git = RealGit::new();
    let repo = repo_git(&ws);
    let head = git
        .run_capture(&repo, &["rev-parse", &config_ref("default")])
        .unwrap();
    let scratch = h.path().join("mint");
    let scratch_s = scratch.to_string_lossy().into_owned();
    git.run(
        &repo,
        &[
            "worktree",
            "add",
            "-b",
            &proposal_ref(id),
            &scratch_s,
            head.trim(),
        ],
    )
    .unwrap();
    std::fs::write(scratch.join("facts.md"), "the box has no network\n").unwrap();
    git.run(&scratch, &["add", "-A"]).unwrap();
    git.run(&scratch, &["commit", "-m", "facts: the box has no network"])
        .unwrap();
    git.run(&repo, &["worktree", "remove", "--force", &scratch_s])
        .unwrap();
    (h, ws)
}

/// The ball's own test: the same write is refused inside a step and
/// lands outside one, and the refused call moves the lineage by nothing.
#[test]
fn a_config_write_from_inside_a_step_is_refused_and_the_lineage_is_unchanged() {
    let (_h, ws) = fixture::workspace();
    let home = TempDir::new().unwrap();
    let before = tip(&ws);
    let (refused, landed) = with_litany_home(home.path(), || {
        let (refused, ..) = with_fx_id("litany", b"", &writing_editor, IN_A_STEP, |fx| {
            config::run(config_args(&ws), fx)
        });
        let (landed, ..) = with_fx_id("litany", b"", &writing_editor, None, |fx| {
            config::run(config_args(&ws), fx)
        });
        (refused, landed)
    });
    let rendered = refused.unwrap_err().to_string();
    assert!(rendered.starts_with("litany config: "), "{rendered}");
    assert!(rendered.contains("LITANY_TOOL_ID"), "{rendered}");
    assert!(
        rendered.contains("litany proposal <workspace>"),
        "{rendered}"
    );
    assert!(
        !ws.join(".config-author").exists(),
        "the refusal precedes the checkout, so nothing is left to heal"
    );
    assert!(matches!(landed.unwrap(), Outcome::Quiet));
    assert_ne!(
        before,
        tip(&ws),
        "the operator's own pass still advances it"
    );
}

/// Acceptance is the veto's other half, so it is refused the same way —
/// and the proposal it refused to accept is still staged.
#[test]
fn accepting_a_proposal_from_inside_a_step_is_refused_and_stages_stand() {
    let (_h, ws) = workspace_with_a_proposal("20260101-a1-s1");
    let before = tip(&ws);
    let (refused, ..) = with_fx_id("litany", b"", &noop_editor, IN_A_STEP, |fx| {
        proposal::run(proposal_args(&ws, Some("20260101-a1-s1"), true, false), fx)
    });
    let rendered = refused.unwrap_err().to_string();
    assert!(rendered.starts_with("litany proposal: "), "{rendered}");
    assert!(
        rendered.contains("accept a proposal onto its lineage"),
        "{rendered}"
    );
    assert_eq!(before, tip(&ws), "no lineage moved");
    assert!(
        RealGit::new()
            .run_capture(
                &repo_git(&ws),
                &["rev-parse", "--verify", &proposal_ref("20260101-a1-s1")],
            )
            .is_ok(),
        "the proposal is still there for the operator"
    );
}

/// What a step may still do: read the staged proposals, read one whole,
/// and reject one. Reading is nobody's risk, and a rejection deletes a
/// branch no lineage points at rather than advancing one.
#[test]
fn a_step_may_still_list_show_and_reject() {
    let (_h, ws) = workspace_with_a_proposal("20260101-a1-s2");
    let before = tip(&ws);
    let run = |args| {
        with_fx_id("litany", b"", &noop_editor, IN_A_STEP, |fx| {
            proposal::run(args, fx)
        })
        .0
    };
    let Outcome::Line(table) = run(proposal_args(&ws, None, false, false)).unwrap() else {
        panic!("the listing is the product")
    };
    assert!(table.contains("20260101-a1-s2"), "{table}");
    let Outcome::Line(shown) =
        run(proposal_args(&ws, Some("20260101-a1-s2"), false, false)).unwrap()
    else {
        panic!("show prints the proposal")
    };
    assert!(shown.contains("+the box has no network"), "{shown}");
    let Outcome::Line(line) = run(proposal_args(&ws, Some("20260101-a1-s2"), false, true)).unwrap()
    else {
        panic!("reject names what it deleted")
    };
    assert!(line.contains("deleted"), "{line}");
    assert_eq!(before, tip(&ws), "a rejection moves no lineage");
}
