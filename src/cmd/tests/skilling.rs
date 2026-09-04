//! `litany skills` at the surface (`docs/DESIGN_LEARNING_LOOP.md` §5):
//! the argv shape, the product, and the two declines that precede any
//! derivation — not a workspace, and a lineage the workspace has not.

use super::{assert_prefixed, noop_editor, with_fx, with_litany_home};
use crate::cmd::{Command, Outcome, skills};
use crate::workspace::fixture;
use std::path::{Path, PathBuf};
use tempfile::TempDir;

/// Parse an argv into its [`Command`], as the binding does.
fn parse(argv: &[&str]) -> Command {
    <crate::cmd::Cli as clap::Parser>::parse_from(argv).command
}

/// Run the verb against a harness root whose data half is `home` — the
/// install pool the census reads as the second skill home.
fn run(home: &Path, args: skills::Args) -> Result<Outcome, crate::cmd::Error> {
    with_litany_home(home, || {
        with_fx("litany", b"", &noop_editor, |fx| skills::run(args, fx)).0
    })
}

#[test]
fn the_argv_shape_is_a_workspace_and_an_optional_lineage() {
    let Command::Skills(bare) = parse(&["litany", "skills", "/ws"]) else {
        panic!("skills takes a workspace")
    };
    assert_eq!(bare.workspace, PathBuf::from("/ws"));
    assert_eq!(bare.config, None);
    let Command::Skills(named) = parse(&["litany", "skills", "/ws", "--config", "alt"]) else {
        panic!("skills takes --config")
    };
    assert_eq!(named.config.as_deref(), Some("alt"));
}

/// The product is the table: headers, then a row per skill both homes
/// offer — here the shipped pool, every entry of it tool-claimed and so
/// active with no election (§5's exemption).
#[test]
fn the_product_is_the_census_table() {
    let (holder, ws) = fixture::workspace();
    let Outcome::Line(table) = run(
        &holder.path().join("data"),
        skills::Args {
            workspace: ws,
            config: None,
        },
    )
    .unwrap() else {
        panic!("skills prints its census")
    };
    let mut lines = table.lines();
    assert!(lines.next().unwrap().starts_with("SKILL "), "{table}");
    let bash = lines
        .find(|l| l.starts_with("bash "))
        .expect("the shipped pool's bash skill has a row");
    assert!(bash.contains("pool"), "{bash}");
    assert!(bash.contains("active"), "{bash}");
}

#[test]
fn a_path_that_is_no_workspace_is_declined() {
    let tmp = TempDir::new().unwrap();
    let e = run(
        tmp.path(),
        skills::Args {
            workspace: tmp.path().join("nope"),
            config: None,
        },
    )
    .unwrap_err();
    assert_prefixed(e, "skills");
}

#[test]
fn a_lineage_the_workspace_has_not_is_declined() {
    let (holder, ws) = fixture::workspace();
    let e = run(
        &holder.path().join("data"),
        skills::Args {
            workspace: ws,
            config: Some("nosuch".into()),
        },
    )
    .unwrap_err();
    assert_prefixed(e, "skills");
}
