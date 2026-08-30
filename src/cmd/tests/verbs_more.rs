//! Product and operator verbs driven against a constructed
//! [`Fx`](crate::cmd::Fx): `scan`, `bundle`, `replay`, `advance`,
//! `tool`, `message`, `stop`. Same discipline as [`super::verbs`]: a hermetic
//! success path where one exists plus a cheap early-error path.
//! `replay`'s product (its scratch path) and the `advance` successor
//! `exec` are pinned by the `tests/*_cli.rs` binary tests.

use super::{assert_prefixed, noop_editor, with_fx};
use crate::cmd::{Outcome, advance, bundle, delete, message, replay, scan, stop, tool};
use crate::workspace::fixture;
use tempfile::TempDir;

#[test]
fn message_advises_on_a_quiescent_branch_whose_latest_call_failed() {
    // bl-ee80: messaging a branch whose latest model call failed stays
    // legal — it is the retry-after-fix path (§2.9-shape resume) — but
    // the verb warns on stderr so the sender learns the branch was not
    // merely idle. Deposit, launch, and exit code are untouched.
    let (_h, ws) = fixture::workspace();
    let agent = "20260101-a9";
    fixture::spawn_root(&ws, agent);
    let step = ws.join("steps").join(agent).join("001");
    std::fs::create_dir_all(&step).unwrap();
    std::fs::write(
        step.join("response.json"),
        "{\"type\":\"error\",\"kind\":\"parse_input\",\"message\":\"bad\"}\n{\"type\":\"end\"}\n",
    )
    .unwrap();
    assert!(message::branch_failed(&ws, agent), "failed shape derived");
    let (r, ..) = with_fx("true", b"", &noop_editor, |fx| {
        message::run(
            message::Args {
                workspace: ws.clone(),
                agent: agent.into(),
                content: "are you there?".into(),
            },
            fx,
        )
    });
    assert!(matches!(r.unwrap(), Outcome::Quiet), "never declined");
    // The advisory names the branch and points at its record.
    let note = message::failed_branch_note(agent);
    assert!(note.contains("latest model call failed"), "{note}");
    assert!(note.contains(&format!("steps/{agent}/")), "{note}");
    assert!(note.contains("litany scan"), "{note}");
}

#[test]
fn scan_prints_its_report() {
    let (_h, ws) = fixture::workspace();
    let (r, ..) = with_fx("true", b"", &noop_editor, |fx| {
        scan::run(scan::Args { workspace: ws }, fx)
    });
    let Outcome::Line(line) = r.unwrap() else {
        panic!("scan prints its report")
    };
    assert!(line.contains("silent deaths"), "{line}");
}

#[test]
fn scan_reports_a_non_workspace() {
    let tmp = TempDir::new().unwrap();
    let (r, ..) = with_fx("true", b"", &noop_editor, |fx| {
        scan::run(
            scan::Args {
                workspace: tmp.path().to_path_buf(),
            },
            fx,
        )
    });
    assert_prefixed(r.unwrap_err(), "scan");
}

#[test]
fn bundle_writes_an_archive() {
    let (_h, ws) = fixture::workspace();
    fixture::spawn_root(&ws, "20260101-a1");
    let out = TempDir::new().unwrap();
    let (r, ..) = with_fx("litany", b"", &noop_editor, |fx| {
        bundle::run(
            bundle::Args {
                workspace: ws.clone(),
                agent: "20260101-a1".into(),
                out_dir: out.path().to_path_buf(),
            },
            fx,
        )
    });
    assert!(matches!(r.unwrap(), Outcome::Quiet));
}

#[test]
fn bundle_reports_an_unknown_agent() {
    let (_h, ws) = fixture::workspace();
    let out = TempDir::new().unwrap();
    let (r, ..) = with_fx("litany", b"", &noop_editor, |fx| {
        bundle::run(
            bundle::Args {
                workspace: ws.clone(),
                agent: "ghost".into(),
                out_dir: out.path().to_path_buf(),
            },
            fx,
        )
    });
    assert_prefixed(r.unwrap_err(), "bundle");
}

#[test]
fn replay_reports_a_missing_archive() {
    let tmp = TempDir::new().unwrap();
    let (r, ..) = with_fx("litany", b"", &noop_editor, |fx| {
        replay::run(
            replay::Args {
                archive: tmp.path().join("nope.bundle"),
            },
            fx,
        )
    });
    assert_prefixed(r.unwrap_err(), "replay");
}

#[test]
fn advance_on_a_quiescent_agent_is_quiet() {
    let (_h, ws) = fixture::workspace();
    fixture::spawn_root(&ws, "20260101-a1");
    let (r, ..) = with_fx("litany", b"", &noop_editor, |fx| {
        advance::run(
            advance::Args {
                workspace: ws.clone(),
                agent: "20260101-a1".into(),
            },
            fx,
        )
    });
    assert!(matches!(r.unwrap(), Outcome::Quiet));
}

#[test]
fn advance_reports_a_non_workspace() {
    let tmp = TempDir::new().unwrap();
    let (r, ..) = with_fx("litany", b"", &noop_editor, |fx| {
        advance::run(
            advance::Args {
                workspace: tmp.path().to_path_buf(),
                agent: "a".into(),
            },
            fx,
        )
    });
    assert_prefixed(r.unwrap_err(), "advance");
}

#[test]
fn tool_runs_a_builtin_and_returns_its_exit_code() {
    let (r, out, _e) = with_fx("litany", br#"{"command":"true"}"#, &noop_editor, |fx| {
        tool::run(
            tool::Args {
                name: "bash".into(),
            },
            fx,
        )
    });
    assert!(matches!(r.unwrap(), Outcome::Code(0)));
    // The bash built-in produced no stdout for `true`.
    assert!(out.is_empty(), "{out:?}");
}

#[test]
fn tool_reports_an_unknown_builtin_with_its_prefix() {
    let (r, ..) = with_fx("litany", b"{}", &noop_editor, |fx| {
        tool::run(
            tool::Args {
                name: "no-such-tool".into(),
            },
            fx,
        )
    });
    assert_prefixed(r.unwrap_err(), "tool no-such-tool");
}

#[test]
fn message_deposits_and_probes() {
    let (_h, ws) = fixture::workspace();
    // The recipient must exist (§2.11): fork its branch first.
    fixture::spawn_root(&ws, "20260101-a1");
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
    assert!(matches!(r.unwrap(), Outcome::Quiet));
}

#[test]
fn message_reports_a_non_workspace() {
    let tmp = TempDir::new().unwrap();
    let (r, ..) = with_fx("true", b"", &noop_editor, |fx| {
        message::run(
            message::Args {
                workspace: tmp.path().to_path_buf(),
                agent: "a".into(),
                content: "c".into(),
            },
            fx,
        )
    });
    assert_prefixed(r.unwrap_err(), "message");
}

#[test]
fn delete_prints_the_census_as_its_product() {
    // The plan and the receipt are one sentence (§9.2): `--dry-run`
    // yields the same [`Outcome::Line`] the real run does, in the
    // conditional mood, having removed nothing.
    let (_h, ws) = fixture::workspace();
    fixture::spawn_root(&ws, "20260101-a1");
    let (r, ..) = with_fx("litany", b"", &noop_editor, |fx| {
        delete::run(
            delete::Args {
                workspace: ws.clone(),
                agent: "20260101-a1".into(),
                children: false,
                dry_run: true,
            },
            fx,
        )
    });
    match r.unwrap() {
        Outcome::Line(l) => assert_eq!(
            l,
            "would delete 20260101-a1; descendants: 0; pending deposits: 0"
        ),
        other => panic!("{other:?}"),
    }
    assert!(
        ws.join("agents/20260101-a1").exists(),
        "dry run removed nothing"
    );
}

#[test]
fn stop_is_idempotent_with_no_executor() {
    let (_h, ws) = fixture::workspace();
    fixture::spawn_root(&ws, "20260101-a1");
    let (r, ..) = with_fx("litany", b"", &noop_editor, |fx| {
        stop::run(
            stop::Args {
                repo: ws.clone(),
                branch: "20260101-a1".into(),
                stop_children: false,
            },
            fx,
        )
    });
    assert!(matches!(r.unwrap(), Outcome::Quiet));
}

#[test]
fn stop_reports_a_non_workspace() {
    let tmp = TempDir::new().unwrap();
    let (r, ..) = with_fx("litany", b"", &noop_editor, |fx| {
        stop::run(
            stop::Args {
                repo: tmp.path().to_path_buf(),
                branch: "b".into(),
                stop_children: false,
            },
            fx,
        )
    });
    assert_prefixed(r.unwrap_err(), "stop");
}
