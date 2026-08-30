//! `retarget` (ARCH §2.2, §3.4): the pre-flights, the mark, and the
//! no-op — all of it before any branch moves, since the landing itself is
//! the executor's and is pinned in `prompt::retarget`.

use super::{assert_prefixed, noop_editor, with_fx};
use crate::cmd::{Outcome, retarget};
use crate::template::{GitRunner, RealGit};
use crate::workspace::{self, fixture};

/// The verb's confirmation is an `eprintln!` on the process's own
/// stderr (§3.4 — like `message`'s advisory), so what a test reads
/// back is the mark itself: the one thing the verb writes.
fn run(ws: &std::path::Path, agent: &str, config: Option<&str>) {
    let (r, out, _err) = with_fx("true", b"", &noop_editor, |fx| {
        retarget::run(
            retarget::Args {
                workspace: ws.to_path_buf(),
                agent: agent.to_string(),
                config: config.map(str::to_string),
            },
            fx,
        )
    });
    assert!(matches!(r.unwrap(), Outcome::Quiet), "product-less (§3.4)");
    assert!(out.is_empty(), "no stdout product (§3.4)");
}

#[test]
fn a_retarget_writes_the_mark_at_the_named_lineages_head() {
    let (_h, ws) = fixture::workspace();
    fixture::spawn_root(&ws, "20260101-a1");
    fixture::amend_config(&ws, &[("souls/worker.md", "an amended soul\n")]);
    let git = RealGit::new();
    let head = git
        .run_capture(&workspace::repo_git(&ws), &["rev-parse", "config/default"])
        .unwrap()
        .trim()
        .to_string();
    run(&ws, "20260101-a1", None);
    assert_eq!(
        workspace::retarget::read(&ws, "20260101-a1", &git),
        Some(head),
        "the mark is the verb's whole effect",
    );
}

#[test]
fn a_target_already_governing_writes_nothing_at_all() {
    let (_h, ws) = fixture::workspace();
    fixture::spawn_root(&ws, "20260101-a1");
    run(&ws, "20260101-a1", Some("default"));
    assert_eq!(
        workspace::retarget::read(&ws, "20260101-a1", &RealGit::new()),
        None,
        "a clean no-op leaves no mark to land",
    );
}

#[test]
fn a_declined_pre_flight_leaves_no_mark_and_renders_the_uniform_failure() {
    let (_h, ws) = fixture::workspace();
    fixture::spawn_root(&ws, "20260101-a1");
    let (r, ..) = with_fx("true", b"", &noop_editor, |fx| {
        retarget::run(
            retarget::Args {
                workspace: ws.clone(),
                agent: "20260101-a1".into(),
                config: Some("nosuch".into()),
            },
            fx,
        )
    });
    assert_prefixed(r.unwrap_err(), "retarget");
    assert_eq!(
        workspace::retarget::read(&ws, "20260101-a1", &RealGit::new()),
        None,
    );
}

#[test]
fn a_mark_that_cannot_be_written_surfaces_the_uniform_failure() {
    // The pre-flights pass against the workspace, then the write is
    // aimed at one that has no repo: the `update-ref` failure arrives
    // as the verb's own `litany retarget: …` line.
    let (_h, ws) = fixture::workspace();
    fixture::spawn_root(&ws, "20260101-a1");
    fixture::amend_config(&ws, &[("souls/worker.md", "an amended soul\n")]);
    std::fs::remove_dir_all(ws.join("repo.git/refs")).unwrap();
    let (r, ..) = with_fx("true", b"", &noop_editor, |fx| {
        retarget::run(
            retarget::Args {
                workspace: ws.clone(),
                agent: "20260101-a1".into(),
                config: None,
            },
            fx,
        )
    });
    assert!(r.is_err());
}
