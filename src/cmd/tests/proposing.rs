//! `litany proposal` at the surface (`docs/DESIGN_LEARNING_LOOP.md` §3):
//! the argv shape, the mode each argument selects, the product of each,
//! and the declines that precede any ref write.

use super::{assert_prefixed, noop_editor, with_fx};
use crate::cmd::{Command, Outcome, proposal};
use crate::template::{GitRunner, RealGit};
use crate::workspace::{config_ref, fixture, proposal::proposal_ref, repo_git};
use std::path::{Path, PathBuf};

/// Parse an argv into its [`Command`], as the binding does.
fn parse(argv: &[&str]) -> Command {
    <crate::cmd::Cli as clap::Parser>::parse_from(argv).command
}

fn run(args: proposal::Args) -> Result<Outcome, crate::cmd::Error> {
    with_fx("litany", b"", &noop_editor, |fx| proposal::run(args, fx)).0
}

fn args(ws: &Path, id: Option<&str>, accept: bool, reject: bool) -> proposal::Args {
    proposal::Args {
        workspace: ws.to_path_buf(),
        id: id.map(str::to_owned),
        accept,
        reject,
    }
}

/// A workspace with one staged proposal on the default lineage.
fn workspace_with_a_proposal(id: &str) -> (tempfile::TempDir, PathBuf) {
    let (h, ws) = fixture::workspace();
    let git = RealGit::new();
    let repo = repo_git(&ws);
    let tip = git
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
            tip.trim(),
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

#[test]
fn the_argv_shape_is_a_workspace_an_optional_id_and_two_exclusive_modes() {
    let Command::Proposal(bare) = parse(&["litany", "proposal", "/ws"]) else {
        panic!("proposal takes a workspace")
    };
    assert_eq!(bare.workspace, PathBuf::from("/ws"));
    assert_eq!(bare.id, None);
    assert!(!bare.accept && !bare.reject);
    let Command::Proposal(one) = parse(&["litany", "proposal", "/ws", "a1-r1", "--accept"]) else {
        panic!("proposal takes an id and a mode")
    };
    assert_eq!(one.id.as_deref(), Some("a1-r1"));
    assert!(one.accept);
    // The two modes are exclusive: a proposal is accepted or rejected,
    // never both, and clap refuses the pair rather than picking one.
    assert!(
        <crate::cmd::Cli as clap::Parser>::try_parse_from([
            "litany", "proposal", "/ws", "a1-r1", "--accept", "--reject",
        ])
        .is_err()
    );
}

#[test]
fn the_bare_form_lists_every_proposal_fresh_or_stale() {
    let (_h, ws) = workspace_with_a_proposal("20260101-a1-r1");
    let Outcome::Line(table) = run(args(&ws, None, false, false)).unwrap() else {
        panic!("the listing is the product")
    };
    let mut lines = table.lines();
    assert!(lines.next().unwrap().starts_with("ID "), "{table}");
    let row = lines.next().expect("one staged proposal");
    assert!(row.starts_with("20260101-a1-r1"), "{row}");
    assert!(row.contains("default"), "{row}");
    assert!(row.contains("fresh"), "{row}");
    assert!(row.contains("facts: the box has no network"), "{row}");
}

#[test]
fn an_id_shows_the_message_and_the_diff() {
    let (_h, ws) = workspace_with_a_proposal("20260101-a1-r2");
    let Outcome::Line(shown) = run(args(&ws, Some("20260101-a1-r2"), false, false)).unwrap() else {
        panic!("show prints the proposal")
    };
    assert!(shown.contains("facts: the box has no network"), "{shown}");
    assert!(shown.contains("+the box has no network"), "{shown}");
}

#[test]
fn accept_moves_the_lineage_and_reject_moves_nothing() {
    let (_h, ws) = workspace_with_a_proposal("20260101-a1-r3");
    let git = RealGit::new();
    let before = git
        .run_capture(&repo_git(&ws), &["rev-parse", &config_ref("default")])
        .unwrap();
    let Outcome::Line(line) = run(args(&ws, Some("20260101-a1-r3"), true, false)).unwrap() else {
        panic!("accept names what moved")
    };
    assert!(line.contains("config/default now stands at"), "{line}");
    let after = git
        .run_capture(&repo_git(&ws), &["rev-parse", &config_ref("default")])
        .unwrap();
    assert_ne!(before, after, "the lineage advanced onto the proposal");

    let (_h2, ws2) = workspace_with_a_proposal("20260101-a1-r4");
    let before2 = git
        .run_capture(&repo_git(&ws2), &["rev-parse", &config_ref("default")])
        .unwrap();
    let Outcome::Line(line) = run(args(&ws2, Some("20260101-a1-r4"), false, true)).unwrap() else {
        panic!("reject names what it deleted")
    };
    assert!(line.contains("deleted"), "{line}");
    assert_eq!(
        before2,
        git.run_capture(&repo_git(&ws2), &["rev-parse", &config_ref("default")])
            .unwrap(),
        "a rejection moves no lineage"
    );
}

#[test]
fn a_mode_without_an_id_is_declined_and_writes_nothing() {
    let (_h, ws) = workspace_with_a_proposal("20260101-a1-r5");
    let err = run(args(&ws, None, true, false)).unwrap_err();
    let rendered = err.to_string();
    assert_prefixed(err, "proposal");
    assert!(rendered.contains("name the proposal"), "{rendered}");
    assert!(
        RealGit::new()
            .run_capture(
                &repo_git(&ws),
                &["rev-parse", "--verify", &proposal_ref("20260101-a1-r5")]
            )
            .is_ok(),
        "the proposal is still staged"
    );
}

#[test]
fn an_id_that_is_not_an_agent_id_is_declined_before_any_ref_is_read() {
    let (_h, ws) = workspace_with_a_proposal("20260101-a1-r6");
    assert_prefixed(
        run(args(&ws, Some("../elsewhere"), false, true)).unwrap_err(),
        "proposal",
    );
}

#[test]
fn a_path_that_is_no_workspace_is_declined() {
    let dir = tempfile::TempDir::new().unwrap();
    assert_prefixed(
        run(args(dir.path(), None, false, false)).unwrap_err(),
        "proposal",
    );
}
