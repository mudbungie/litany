//! The agent-id guard on the command surface (ARCH §2.3): every verb
//! taking an id from outside declines one that is not a single path
//! component, before it can reach a `join`. One test per verb — each
//! owns its own failure prefix (§3.4) — plus the traversal and absolute
//! shapes `Path::join` would otherwise honour: `..` walks out of the
//! workspace, and an absolute id *replaces* it.

use super::{assert_prefixed, noop_editor, with_fx};
use crate::cmd::{advance, bundle, delete, dispatch, message, retarget, stop, workflow};
use crate::workspace::fixture;
use std::path::PathBuf;
use tempfile::TempDir;

/// The audit's repro shape: an id that climbs out of `<ws>/inbox/`.
const ESCAPING: &str = "../../victim/pwned";

#[test]
fn message_declines_an_escaping_id_and_writes_nothing_outside_the_workspace() {
    let (holder, ws) = fixture::workspace();
    let (r, ..) = with_fx("true", b"", &noop_editor, |fx| {
        message::run(
            message::Args {
                workspace: ws.clone(),
                agent: ESCAPING.into(),
                content: "hi".into(),
            },
            fx,
        )
    });
    assert_prefixed(r.unwrap_err(), "message");
    // `<ws>/inbox/../../victim` is `<holder>/victim` — the write the
    // unguarded join would have made.
    assert!(
        !holder.path().join("victim").exists(),
        "a declined id writes nothing outside the workspace"
    );
}

#[test]
fn message_declines_an_absolute_id_that_would_replace_the_workspace_base() {
    let (holder, ws) = fixture::workspace();
    let absolute = holder.path().join("pwned");
    let (r, ..) = with_fx("true", b"", &noop_editor, |fx| {
        message::run(
            message::Args {
                workspace: ws.clone(),
                agent: absolute.display().to_string(),
                content: "hi".into(),
            },
            fx,
        )
    });
    assert_prefixed(r.unwrap_err(), "message");
    assert!(!absolute.exists(), "an absolute id is declined, not joined");
}

#[test]
fn message_declines_an_agent_that_does_not_exist() {
    let (_h, ws) = fixture::workspace();
    let (r, ..) = with_fx("true", b"", &noop_editor, |fx| {
        message::run(
            message::Args {
                workspace: ws.clone(),
                agent: "20260101-a1".into(),
                content: "hi".into(),
            },
            fx,
        )
    });
    let err = r.unwrap_err().to_string();
    assert!(err.contains("no agent \"20260101-a1\""), "{err}");
    assert!(
        !ws.join("inbox").join("20260101-a1").exists(),
        "the decline creates no inbox directory"
    );
}

#[test]
fn advance_declines_an_escaping_id() {
    let tmp = TempDir::new().unwrap();
    let (r, ..) = with_fx("true", b"", &noop_editor, |fx| {
        advance::run(
            advance::Args {
                workspace: tmp.path().to_path_buf(),
                agent: ESCAPING.into(),
            },
            fx,
        )
    });
    assert_prefixed(r.unwrap_err(), "advance");
    assert!(
        !tmp.path().join("inbox").exists(),
        "the driver takes no lease behind a declined id"
    );
}

#[test]
fn advance_declines_an_agent_that_does_not_exist() {
    // The existence half of the guard (§2.3), which `advance` lacked:
    // a hop at a name with no `agents/*` ref exited 0 in silence and
    // left `inbox/<name>/` behind — the orphan `litany scan` reports as
    // debris, manufactured by an operator typo.
    let (_h, ws) = fixture::workspace();
    let (r, ..) = with_fx("litany", b"", &noop_editor, |fx| {
        advance::run(
            advance::Args {
                workspace: ws.clone(),
                agent: "ghost".into(),
            },
            fx,
        )
    });
    assert_eq!(
        r.unwrap_err().to_string(),
        "litany advance: no agent \"ghost\" in this workspace — a hop drives an existing \
         agent (ARCH §2.3: the `agents/*` refs are the registry); check the id against the \
         workspace's `agents/*` refs, or start an agent with `litany prompt` / `litany dispatch`"
    );
    assert!(
        !ws.join("inbox").join("ghost").exists(),
        "the decline creates no inbox directory"
    );
}

#[test]
fn stop_declines_an_escaping_id() {
    let tmp = TempDir::new().unwrap();
    let (r, ..) = with_fx("true", b"", &noop_editor, |fx| {
        stop::run(
            stop::Args {
                repo: tmp.path().to_path_buf(),
                branch: ESCAPING.into(),
                stop_children: false,
            },
            fx,
        )
    });
    assert_prefixed(r.unwrap_err(), "stop");
}

#[test]
fn dispatch_declines_an_escaping_parent_id() {
    let tmp = TempDir::new().unwrap();
    let (r, ..) = with_fx("true", b"", &noop_editor, |fx| {
        dispatch::run(
            dispatch::Args {
                role: "worker".into(),
                repo: tmp.path().to_path_buf(),
                branch: ESCAPING.into(),
                goal: Some("g".into()),
                from: None,
                name: None,
                pin: vec![],
                cwd: None,
            },
            fx,
        )
    });
    assert_prefixed(r.unwrap_err(), "dispatch worker");
}

#[test]
fn bundle_declines_an_escaping_id() {
    let tmp = TempDir::new().unwrap();
    let (r, ..) = with_fx("true", b"", &noop_editor, |fx| {
        bundle::run(
            bundle::Args {
                workspace: tmp.path().to_path_buf(),
                agent: ESCAPING.into(),
                out_dir: PathBuf::from("/dev/null/never"),
            },
            fx,
        )
    });
    assert_prefixed(r.unwrap_err(), "bundle");
}

#[test]
fn delete_declines_an_escaping_id() {
    let tmp = TempDir::new().unwrap();
    let (r, ..) = with_fx("true", b"", &noop_editor, |fx| {
        delete::run(
            delete::Args {
                workspace: tmp.path().to_path_buf(),
                agent: ESCAPING.into(),
                children: false,
                dry_run: false,
            },
            fx,
        )
    });
    assert_prefixed(r.unwrap_err(), "delete");
    assert!(
        !tmp.path().join("inbox").exists(),
        "a declined id probes no lock and creates nothing"
    );
}

#[test]
fn retarget_declines_an_escaping_id_before_it_reaches_a_ref() {
    let tmp = TempDir::new().unwrap();
    let (r, ..) = with_fx("true", b"", &noop_editor, |fx| {
        retarget::run(
            retarget::Args {
                workspace: tmp.path().to_path_buf(),
                agent: ESCAPING.into(),
                config: None,
            },
            fx,
        )
    });
    assert_prefixed(r.unwrap_err(), "retarget");
}

#[test]
fn workflow_declines_an_escaping_id_before_it_reaches_a_ref() {
    let tmp = TempDir::new().unwrap();
    let (r, ..) = with_fx("true", b"", &noop_editor, |fx| {
        workflow::run(
            workflow::Args {
                workspace: tmp.path().to_path_buf(),
                agent: ESCAPING.into(),
                config: None,
                clear: false,
            },
            fx,
        )
    });
    assert_prefixed(r.unwrap_err(), "workflow");
}
