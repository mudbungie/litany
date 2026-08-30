//! Authoring and driver verbs driven against a constructed
//! [`Fx`](crate::cmd::Fx): `new`, `config`, `prompt`, `dispatch`,
//! `stop`. Each has a hermetic success path where one exists, plus a
//! cheap early-error path pinning the one-conversion failure shape
//! (`litany <prefix>: …`). Detached launches use `"true"` as the driver
//! target (spawned, harmless). `message`'s pair lives in
//! [`super::verbs_more`] beside its state-derivation edge case.

use super::{assert_prefixed, noop_editor, with_fx, with_litany_home, writing_editor};
use crate::cmd::{Outcome, config, dispatch, new, prompt};
use crate::template::{GitRunner, RealGit};
use crate::workspace::{fixture, repo_git};
use std::path::Path;
use tempfile::TempDir;

/// Run `litany new <dest>` against a scratch harness root — the verb
/// founds the root it resolves (§2.2), so every in-process run must
/// point `LITANY_HOME` away from the developer's own install.
fn run_new(home: &Path, dest: &Path) -> Result<Outcome, crate::cmd::Error> {
    with_litany_home(home, || {
        with_fx("litany", b"", &noop_editor, |fx| {
            new::run(
                new::Args {
                    path: Some(dest.to_path_buf()),
                },
                fx,
            )
        })
        .0
    })
}

/// Every path in the workspace's first config commit.
fn config_tree(dest: &Path) -> String {
    RealGit::new()
        .run_capture(
            &repo_git(dest),
            &["ls-tree", "-r", "--name-only", "config/default"],
        )
        .unwrap()
}

#[test]
fn new_scaffolds_and_prints_the_destination() {
    let home = TempDir::new().unwrap();
    let tmp = TempDir::new().unwrap();
    let dest = tmp.path().join("ws");
    let Outcome::Line(line) = run_new(home.path(), &dest).unwrap() else {
        panic!("new prints its destination")
    };
    assert_eq!(line, dest.display().to_string());
    assert!(dest.join("repo.git").is_dir());
}

#[test]
fn new_founds_an_unseeded_root_so_the_config_commit_carries_descriptions() {
    // A fresh `LITANY_HOME` with no `litany prime` run against it: the
    // verb founds the pools through prime's own seed-if-absent routine
    // (§2.2), so the first config commit carries the control files *and*
    // a populated `descriptions/**` (§3.3 descriptions-always) instead
    // of silently authoring a toolless config.
    let home = TempDir::new().unwrap();
    let tmp = TempDir::new().unwrap();
    let dest = tmp.path().join("ws");
    run_new(home.path(), &dest).unwrap();

    // The pools were founded under the previously-empty home …
    assert!(home.path().join("tools/read_file.json").is_file());
    assert!(home.path().join("skills/read_file/SKILL.md").is_file());
    // … and the commit's tree carries the snapshot beside the controls.
    let tree = config_tree(&dest);
    for path in [
        "manifest.yaml",
        "providers.yaml",
        "workflow.yaml",
        "version",
        "descriptions/tools/read_file.json",
        "descriptions/skills/read_file.md",
    ] {
        assert!(tree.contains(path), "{path} missing from:\n{tree}");
    }
}

#[test]
fn new_over_a_seeded_root_keeps_the_curated_pool_entry() {
    // Founding never clobbers (§2.2 seed-if-absent), so a hand-edited
    // pool entry is what the config commit snapshots.
    let home = TempDir::new().unwrap();
    std::fs::create_dir_all(home.path().join("tools")).unwrap();
    std::fs::write(home.path().join("tools/read_file.json"), r#"{"mine":1}"#).unwrap();
    let tmp = TempDir::new().unwrap();
    let dest = tmp.path().join("ws");
    run_new(home.path(), &dest).unwrap();

    assert_eq!(
        std::fs::read_to_string(home.path().join("tools/read_file.json")).unwrap(),
        r#"{"mine":1}"#
    );
    let shown = RealGit::new()
        .run_capture(
            &repo_git(&dest),
            &["show", "config/default:descriptions/tools/read_file.json"],
        )
        .unwrap();
    assert_eq!(shown, r#"{"mine":1}"#);
}

#[test]
fn new_reports_a_scaffold_failure() {
    let home = TempDir::new().unwrap();
    let tmp = TempDir::new().unwrap();
    let dest = tmp.path().join("ws");
    std::fs::create_dir_all(&dest).unwrap();
    std::fs::write(dest.join("occupied"), b"x").unwrap();
    assert_prefixed(run_new(home.path(), &dest).unwrap_err(), "new");
}

#[test]
fn new_at_an_existing_plain_file_names_the_rule_not_the_errno() {
    // The adjacent guard case names the path and the rule ("already
    // exists and is not empty"); a plain file must get the same voice
    // rather than a bare `os error 20`, and nothing gets written.
    let home = TempDir::new().unwrap();
    let tmp = TempDir::new().unwrap();
    let dest = tmp.path().join("afile");
    std::fs::write(&dest, b"x").unwrap();
    let err = run_new(home.path(), &dest).unwrap_err();
    assert_eq!(
        err.to_string(),
        format!(
            "litany new: destination {} already exists and is not a directory",
            dest.display()
        )
    );
    assert_eq!(std::fs::read(&dest).unwrap(), b"x");
}

#[test]
fn config_authors_a_commit() {
    // `config::run` resolves the harness root itself, unmocked — a
    // scratch `LITANY_HOME` (§2.2, process-global) keeps it off the real
    // install and off other tests' own scratch homes.
    let (_h, ws) = fixture::workspace();
    let args = config::Args {
        workspace: ws,
        name: None,
        from: None,
        orphan: false,
    };
    let home = TempDir::new().unwrap();
    let (r, ..) = with_litany_home(home.path(), || {
        with_fx("litany", b"", &writing_editor, |fx| config::run(args, fx))
    });
    assert!(matches!(r.unwrap(), Outcome::Quiet));
}

#[test]
fn config_reports_a_declined_pass_as_a_clean_line() {
    // An editor that saves nothing, twice: the first pass still lands
    // the §3.3 descriptions refresh, the second changes nothing at all
    // and is declined — a success (no error, no wedged checkout) that
    // names the branch that did not move. Both passes share one scratch
    // `LITANY_HOME` (see `config_authors_a_commit`) so the two resolves
    // see the same data root instead of racing another test's window.
    let (_h, ws) = fixture::workspace();
    let args = || config::Args {
        workspace: ws.clone(),
        name: None,
        from: None,
        orphan: false,
    };
    let home = TempDir::new().unwrap();
    let line = with_litany_home(home.path(), || {
        let (first, ..) = with_fx("litany", b"", &noop_editor, |fx| config::run(args(), fx));
        first.unwrap();
        let (r, ..) = with_fx("litany", b"", &noop_editor, |fx| config::run(args(), fx));
        let Outcome::Line(line) = r.unwrap() else {
            panic!("a declined pass reports the branch that did not move")
        };
        line
    });
    assert!(line.starts_with("config/default unchanged: "), "{line}");
    assert!(!ws.join(".config-author").exists());
}

#[test]
fn config_reports_a_non_workspace() {
    let tmp = TempDir::new().unwrap();
    let (r, ..) = with_fx("litany", b"", &noop_editor, |fx| {
        config::run(
            config::Args {
                workspace: tmp.path().to_path_buf(),
                name: None,
                from: None,
                orphan: false,
            },
            fx,
        )
    });
    assert_prefixed(r.unwrap_err(), "config");
}

#[test]
fn prompt_reports_a_non_workspace() {
    let tmp = TempDir::new().unwrap();
    let (r, ..) = with_fx("litany", b"", &noop_editor, |fx| {
        prompt::run(
            prompt::Args {
                repo: tmp.path().to_path_buf(),
                message: "hi".into(),
                from: None,
                config: None,
                name: None,
                pin: vec![],
                cwd: None,
            },
            fx,
        )
    });
    assert_prefixed(r.unwrap_err(), "prompt");
}

#[test]
fn dispatch_forks_a_child_through_the_front_door() {
    let (_h, ws) = fixture::workspace();
    fixture::spawn_root(&ws, "20260101-p1");
    let (r, ..) = with_fx("true", b"", &noop_editor, |fx| {
        dispatch::run(
            dispatch::Args {
                role: "worker".into(),
                repo: ws.clone(),
                branch: "20260101-p1".into(),
                goal: Some("do the thing".into()),
                from: None,
                name: None,
                pin: vec![],
                cwd: None,
            },
            fx,
        )
    });
    assert!(matches!(r.unwrap(), Outcome::Quiet));
}

#[test]
fn dispatch_reports_an_undefined_role_with_its_prefix() {
    let (_h, ws) = fixture::workspace();
    fixture::spawn_root(&ws, "p1");
    let (r, ..) = with_fx("true", b"", &noop_editor, |fx| {
        dispatch::run(
            dispatch::Args {
                role: "no-such".into(),
                repo: ws.clone(),
                branch: "p1".into(),
                goal: Some("g".into()),
                from: None,
                name: None,
                pin: vec![],
                cwd: None,
            },
            fx,
        )
    });
    assert_prefixed(r.unwrap_err(), "dispatch no-such");
}

// `message`'s success/non-workspace pair and `stop`'s idempotence /
// non-workspace pair live in `verbs_more.rs` — whole verbs move there
// when this file nears the 300-line cap, never a split mid-verb.
