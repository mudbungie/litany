//! Election over the **two skill homes** (`docs/DESIGN_LEARNING_LOOP.md`
//! §3, ARCH §3.3): the followed config commit's workspace skills first,
//! the install pool second. One test per branch of that resolution, so
//! a coverage regression names the path it broke.

use super::*;
use crate::template::RealGit;
use crate::workspace::fixture;
use std::collections::HashMap;
use std::ffi::OsString;
use std::io::Cursor;
use std::path::{Path, PathBuf};
use tempfile::TempDir;

struct StubEnv(HashMap<&'static str, OsString>);
impl EnvLookup for StubEnv {
    fn get(&self, key: &str) -> Option<OsString> {
        self.0.get(key).cloned()
    }
}

fn env(repo: &Path, home: &Path) -> StubEnv {
    let mut m = HashMap::new();
    m.insert(ENV_CONV_REPO, repo.as_os_str().to_owned());
    m.insert(ENV_CONV_BRANCH, OsString::from("a1"));
    m.insert(ENV_LITANY_HOME, home.as_os_str().to_owned());
    StubEnv(m)
}

fn input(name: &str) -> Cursor<Vec<u8>> {
    Cursor::new(serde_json::json!({ "name": name }).to_string().into_bytes())
}

/// A `SKILL.md` the descriptions snapshot's parser accepts.
fn manifest(name: &str, body: &str) -> String {
    format!("---\nname: {name}\ndescription: d\n---\n{body}")
}

/// A root agent `a1` whose lineage gained the given files **after** the
/// fork — follow-the-tip (ARCH §2.2), and the shape an accepted config
/// edit reaches a live agent in. Amending after the fork is also what
/// keeps the worktree free of the bodies: production's dispatch commit
/// trims `skills/` out of a tree forked off a config commit
/// (`crate::prompt::dispatch::trim_to_context`), and this fixture forks
/// off a commit that had none.
fn workspace_with(files: &[(&str, &str)]) -> (TempDir, PathBuf) {
    let (holder, ws) = fixture::workspace();
    fixture::spawn_root(&ws, "a1");
    fixture::amend_config(&ws, files);
    (holder, ws)
}

fn pool_skill(home: &Path, name: &str, body: &str) {
    let dir = home.join("skills").join(name);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("SKILL.md"), manifest(name, body)).unwrap();
}

fn staged(ws: &Path) -> String {
    RealGit::new()
        .run_capture(&ws.join("agents/a1"), &["diff", "--cached", "--name-only"])
        .unwrap()
}

#[test]
fn a_workspace_skill_is_checked_out_of_the_followed_config_commit() {
    let notes = manifest("notes", "the workspace's own lesson");
    let (_h, ws) = workspace_with(&[("skills/notes/SKILL.md", notes.as_str())]);
    let home = TempDir::new().unwrap();
    let mut out = Vec::new();
    run(&mut input("notes"), &mut out, &env(&ws, home.path())).unwrap();

    let payload: serde_json::Value = serde_json::from_slice(&out).unwrap();
    assert_eq!(payload["status"], "loaded");
    assert_eq!(payload["path"], "skills/notes");
    let dest = ws.join("agents/a1/skills/notes/SKILL.md");
    assert_eq!(std::fs::read_to_string(&dest).unwrap(), notes);
    // `git checkout <commit> -- <path>` writes *and* stages, so the
    // tool's own commit carries the body with no second `add`.
    assert!(
        staged(&ws).contains("skills/notes/SKILL.md"),
        "{}",
        staged(&ws)
    );
}

#[test]
fn a_name_both_homes_hold_loads_the_followed_tips_body() {
    // The authoring pass refuses this state when it can see the pool;
    // a pool that gained the name afterwards is the drift the ordering
    // rule exists for, and the lineage's own body is what wins.
    let ours = manifest("notes", "the workspace's own");
    let (_h, ws) = workspace_with(&[("skills/notes/SKILL.md", ours.as_str())]);
    let home = TempDir::new().unwrap();
    pool_skill(home.path(), "notes", "the install's");

    let mut out = Vec::new();
    run(&mut input("notes"), &mut out, &env(&ws, home.path())).unwrap();
    let dest = ws.join("agents/a1/skills/notes/SKILL.md");
    assert_eq!(std::fs::read_to_string(&dest).unwrap(), ours);
}

#[test]
fn a_name_neither_home_holds_is_declined_naming_both() {
    let notes = manifest("notes", "ours");
    let (_h, ws) = workspace_with(&[("skills/notes/SKILL.md", notes.as_str())]);
    let home = TempDir::new().unwrap();
    pool_skill(home.path(), "git-ops", "theirs");

    let err = run(&mut input("nope"), &mut Vec::new(), &env(&ws, home.path())).unwrap_err();
    let msg = err.to_string();
    assert!(matches!(err, Error::Unknown { .. }), "{msg}");
    assert!(msg.contains("notes"), "{msg}");
    assert!(msg.contains("git-ops"), "{msg}");
}

#[test]
fn the_archive_is_neither_listed_nor_loadable() {
    let old = manifest("old", "retired");
    let notes = manifest("notes", "ours");
    let (_h, ws) = workspace_with(&[
        ("skills/notes/SKILL.md", notes.as_str()),
        ("skills/archived/old/SKILL.md", old.as_str()),
    ]);
    let home = TempDir::new().unwrap();

    // The container is a directory in the commit, so a name check is
    // what stops an election from dragging the whole archive in.
    let err = run(
        &mut input("archived"),
        &mut Vec::new(),
        &env(&ws, home.path()),
    )
    .unwrap_err();
    assert!(matches!(err, Error::Archived(_)), "{err}");
    // `archived/old` never reaches a home: it is not one path component.
    let err = run(
        &mut input("archived/old"),
        &mut Vec::new(),
        &env(&ws, home.path()),
    )
    .unwrap_err();
    assert!(matches!(err, Error::BadName(_)), "{err}");
    // And the container is not named among the workspace's skills.
    let err = run(&mut input("nope"), &mut Vec::new(), &env(&ws, home.path())).unwrap_err();
    let msg = err.to_string();
    assert!(msg.contains("notes"), "{msg}");
    assert!(!msg.contains("archived"), "{msg}");
}

#[test]
fn a_branch_with_no_config_ancestry_declines_naming_the_lineage() {
    let repo = TempDir::new().unwrap();
    let home = TempDir::new().unwrap();
    let err = run(
        &mut input("notes"),
        &mut Vec::new(),
        &env(repo.path(), home.path()),
    )
    .unwrap_err();
    assert!(matches!(err, Error::Lineage(_)), "{err}");
}

#[test]
fn a_failed_checkout_of_a_workspace_skill_surfaces_checkout() {
    /// Real git for every probe, refusing only the checkout — the one
    /// shape a real repository cannot be asked to produce on demand.
    struct RefusesCheckout(RealGit);
    impl GitRunner for RefusesCheckout {
        fn run(&self, dest: &Path, args: &[&str]) -> std::io::Result<()> {
            if args.first() == Some(&"checkout") {
                return Err(std::io::Error::other("checkout refused"));
            }
            self.0.run(dest, args)
        }
        fn run_capture(&self, dest: &Path, args: &[&str]) -> std::io::Result<String> {
            self.0.run_capture(dest, args)
        }
    }

    let notes = manifest("notes", "ours");
    let (_h, ws) = workspace_with(&[("skills/notes/SKILL.md", notes.as_str())]);
    let home = TempDir::new().unwrap();
    let err = run_with(
        &mut input("notes"),
        &mut Vec::new(),
        &env(&ws, home.path()),
        &RefusesCheckout(RealGit::new()),
    )
    .unwrap_err();
    assert!(matches!(err, Error::Checkout { .. }), "{err}");
}
