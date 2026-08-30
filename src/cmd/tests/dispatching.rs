//! [`Command::run`](crate::cmd::Command::run) dispatch coverage: every
//! arm of the exhaustive match (§3.4) exercised through the parsed
//! `Command`, plus `prime` — whose success seeds real files and so is
//! driven against a scratch `LITANY_HOME`, and whose failure pins the
//! one-conversion `litany prime: …` shape. `new` founds the harness root
//! through prime's routine (§2.2), so it runs against a scratch home too.

use super::{assert_prefixed, noop_editor, with_fx, with_litany_home};
use crate::cmd::{
    Command, Outcome, advance, bundle, config, dispatch, message, new, prime, prompt, replay,
    retarget, scan, stop, tool,
};
use tempfile::TempDir;

/// Drive one `Command` through the exhaustive `run` match with scratch
/// stdio; `true` is the (unspawned) driver target. Returns `Ok`-ness.
fn dispatched(cmd: Command) -> bool {
    with_fx("true", b"{}", &noop_editor, |fx| cmd.run(fx))
        .0
        .is_ok()
}

#[test]
fn command_run_dispatches_every_non_prime_arm() {
    let d = TempDir::new().unwrap();
    // A non-workspace path — every driver/archive verb hits its cheap
    // early guard against it; `new` scaffolds a fresh scratch dest.
    let ne = || d.path().to_path_buf();
    let home = TempDir::new().unwrap();
    assert!(with_litany_home(home.path(), || dispatched(Command::New(
        new::Args {
            path: Some(d.path().join("w")),
        }
    ))));
    assert!(!dispatched(Command::Config(config::Args {
        workspace: ne(),
        name: None,
        from: None,
        orphan: false,
    })));
    assert!(!dispatched(Command::Prompt(prompt::Args {
        repo: ne(),
        message: "m".into(),
        from: None,
        config: None,
        name: None,
        pin: vec![],
        cwd: None,
    })));
    assert!(!dispatched(Command::Dispatch(dispatch::Args {
        role: "worker".into(),
        repo: ne(),
        branch: "b".into(),
        goal: Some("g".into()),
        from: None,
        name: None,
        pin: vec![],
        cwd: None,
    })));
    assert!(!dispatched(Command::Retarget(retarget::Args {
        workspace: ne(),
        agent: "a".into(),
        config: None,
    })));
    assert!(!dispatched(Command::Stop(stop::Args {
        repo: ne(),
        branch: "b".into(),
        stop_children: false,
    })));
    assert!(!dispatched(Command::Message(message::Args {
        workspace: ne(),
        agent: "a".into(),
        content: "c".into(),
    })));
    assert!(!dispatched(Command::Scan(scan::Args { workspace: ne() })));
    assert!(!dispatched(Command::Bundle(bundle::Args {
        workspace: ne(),
        agent: "a".into(),
        out_dir: d.path().join("o"),
    })));
    assert!(!dispatched(Command::Replay(replay::Args {
        archive: d.path().join("none.bundle"),
    })));
    assert!(!dispatched(Command::Advance(advance::Args {
        workspace: ne(),
        agent: "a".into(),
    })));
    assert!(!dispatched(Command::Tool(tool::Args {
        name: "no-such".into(),
    })));
}

#[test]
fn command_run_primes_a_scratch_home() {
    let home = TempDir::new().unwrap();
    let out = with_litany_home(home.path(), || {
        with_fx("true", b"", &noop_editor, |fx| {
            Command::Prime(prime::Args {}).run(fx)
        })
        .0
    });
    assert!(matches!(out.unwrap(), Outcome::Quiet));
    assert!(home.path().join("workspaces").is_dir());
}

#[test]
fn prime_reports_a_seeding_failure() {
    let tmp = TempDir::new().unwrap();
    let file = tmp.path().join("not-a-dir");
    std::fs::write(&file, b"x").unwrap();
    let err = with_litany_home(&file.join("home"), || {
        with_fx("true", b"", &noop_editor, |fx| {
            prime::run(prime::Args {}, fx)
        })
        .0
    })
    .unwrap_err();
    assert_prefixed(err, "prime");
}
