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
    with_role(ws, agent, config, None);
}

/// The same, naming a role (§4.3, bl-946c).
fn with_role(ws: &std::path::Path, agent: &str, config: Option<&str>, role: Option<&str>) {
    let (r, out, _err) = with_fx("true", b"", &noop_editor, |fx| {
        retarget::run(
            retarget::Args {
                workspace: ws.to_path_buf(),
                agent: agent.to_string(),
                config: config.map(str::to_string),
                role: role.map(str::to_string),
            },
            fx,
        )
    });
    assert!(matches!(r.unwrap(), Outcome::Quiet), "product-less (§3.4)");
    assert!(out.is_empty(), "no stdout product (§3.4)");
}

#[test]
fn a_role_names_the_role_mark_and_nothing_else_when_the_lineage_is_unmoved() {
    // §4.3 / bl-946c: plan mode for a whole running conversation is one
    // gesture and one mark — the config the agent already resolves stays
    // where it is.
    let (_h, ws) = fixture::workspace();
    fixture::spawn_root(&ws, "20260101-a1");
    let git = RealGit::new();
    with_role(&ws, "20260101-a1", None, Some("planner"));
    assert_eq!(
        workspace::retarget::read_role(&ws, "20260101-a1", &git),
        Some("planner".to_string()),
    );
    assert_eq!(
        workspace::retarget::read(&ws, "20260101-a1", &git),
        None,
        "an unmoved lineage writes no config mark",
    );
}

#[test]
fn the_role_the_agent_already_carries_writes_no_mark_at_all() {
    let (_h, ws) = fixture::workspace();
    fixture::spawn_root(&ws, "20260101-a1");
    with_role(&ws, "20260101-a1", None, Some("worker"));
    assert_eq!(
        workspace::retarget::read_role(&ws, "20260101-a1", &RealGit::new()),
        None,
    );
}

#[test]
fn a_role_the_config_does_not_declare_is_refused_before_any_mark() {
    let (_h, ws) = fixture::workspace();
    fixture::spawn_root(&ws, "20260101-a1");
    let (r, _out, _err) = with_fx("true", b"", &noop_editor, |fx| {
        retarget::run(
            retarget::Args {
                workspace: ws.clone(),
                agent: "20260101-a1".into(),
                config: None,
                role: Some("ghost".into()),
            },
            fx,
        )
    });
    assert_prefixed(r.unwrap_err(), "retarget");
    assert_eq!(
        workspace::retarget::read_role(&ws, "20260101-a1", &RealGit::new()),
        None,
    );
}

#[test]
fn a_retarget_writes_the_mark_at_the_named_lineages_head() {
    // A diverged lineage: under follow-the-tip (§2.2, bl-403b) a
    // same-lineage advance is already the agent's next resolution, so
    // what still marks is a change of lineage.
    let (_h, ws) = fixture::workspace();
    fixture::spawn_root(&ws, "20260101-a1");
    let git = RealGit::new();
    let fork = git
        .run_capture(&workspace::repo_git(&ws), &["rev-parse", "config/default"])
        .unwrap()
        .trim()
        .to_string();
    git.run(
        &workspace::repo_git(&ws),
        &["update-ref", "refs/heads/config/variant", &fork],
    )
    .unwrap();
    fixture::amend_lineage(&ws, "variant", &[("souls/worker.md", "an amended soul\n")]);
    let head = git
        .run_capture(&workspace::repo_git(&ws), &["rev-parse", "config/variant"])
        .unwrap()
        .trim()
        .to_string();
    run(&ws, "20260101-a1", Some("variant"));
    assert_eq!(
        workspace::retarget::read(&ws, "20260101-a1", &git),
        Some(head),
        "the mark is the verb's whole effect",
    );
}

#[test]
fn a_retarget_onto_the_agents_own_advanced_lineage_is_a_clean_no_op() {
    // The inverted freeze pin (bl-403b): the agent's next step reads the
    // advanced head anyway, so nothing is marked.
    let (_h, ws) = fixture::workspace();
    fixture::spawn_root(&ws, "20260101-a1");
    fixture::amend_config(&ws, &[("souls/worker.md", "an amended soul\n")]);
    run(&ws, "20260101-a1", None);
    assert_eq!(
        workspace::retarget::read(&ws, "20260101-a1", &RealGit::new()),
        None,
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
                role: None,
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
                role: None,
            },
            fx,
        )
    });
    assert!(r.is_err());
}
