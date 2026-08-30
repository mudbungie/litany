//! Unit tests for [`super::run`] (ARCH §3.3 *Body-on-demand*). Each
//! branch and every error variant lands in its own test so a coverage
//! regression points at the offending path.

use super::*;
use std::collections::HashMap;
use std::ffi::OsString;
use std::io::Cursor;
use tempfile::TempDir;

/// HashMap-backed stub [`EnvLookup`] — `None` for anything not seeded.
struct StubEnv(HashMap<&'static str, OsString>);
impl EnvLookup for StubEnv {
    fn get(&self, key: &str) -> Option<OsString> {
        self.0.get(key).cloned()
    }
}

/// Env with workspace + branch + a `LITANY_HOME`-collapsed data root.
fn env(repo: &Path, branch: &str, home: &Path) -> StubEnv {
    let mut m = HashMap::new();
    m.insert(ENV_CONV_REPO, repo.as_os_str().to_owned());
    m.insert(ENV_CONV_BRANCH, OsString::from(branch));
    m.insert(ENV_LITANY_HOME, home.as_os_str().to_owned());
    StubEnv(m)
}

/// Seed a skill `name` in `home`'s pool with a top-level file and a
/// nested dir+file, so a copy exercises both `copy_dir` arms.
fn seed_skill(home: &Path, name: &str) {
    let dir = home.join("skills").join(name);
    std::fs::create_dir_all(dir.join("refs")).unwrap();
    std::fs::write(dir.join("SKILL.md"), b"---\nname: x\n---\nbody").unwrap();
    std::fs::write(dir.join("refs/extra.md"), b"more").unwrap();
}

fn input(name: &str) -> Cursor<Vec<u8>> {
    Cursor::new(serde_json::json!({ "name": name }).to_string().into_bytes())
}

#[test]
fn happy_path_copies_body_into_worktree_and_reports_loaded() {
    let repo = TempDir::new().unwrap();
    let home = TempDir::new().unwrap();
    seed_skill(home.path(), "git-ops");
    let mut out = Vec::new();
    run(
        &mut input("git-ops"),
        &mut out,
        &env(repo.path(), "a1", home.path()),
    )
    .unwrap();

    let payload: serde_json::Value = serde_json::from_slice(&out).unwrap();
    assert_eq!(payload["status"], "loaded");
    assert_eq!(payload["path"], "skills/git-ops");
    // Copy, not symlink — the whole tree is materialized in the worktree.
    let dest = repo.path().join("agents/a1/skills/git-ops");
    assert_eq!(
        std::fs::read_to_string(dest.join("SKILL.md")).unwrap(),
        "---\nname: x\n---\nbody"
    );
    assert_eq!(
        std::fs::read_to_string(dest.join("refs/extra.md")).unwrap(),
        "more"
    );
}

#[test]
fn already_loaded_is_idempotent_and_leaves_the_copy_untouched() {
    let repo = TempDir::new().unwrap();
    let home = TempDir::new().unwrap();
    // A worktree copy exists with content the pool does *not* have.
    let dest = repo.path().join("agents/a1/skills/git-ops");
    std::fs::create_dir_all(&dest).unwrap();
    std::fs::write(dest.join("SKILL.md"), b"pinned").unwrap();
    seed_skill(home.path(), "git-ops"); // pool differs — must not win.

    let mut out = Vec::new();
    run(
        &mut input("git-ops"),
        &mut out,
        &env(repo.path(), "a1", home.path()),
    )
    .unwrap();

    let payload: serde_json::Value = serde_json::from_slice(&out).unwrap();
    assert_eq!(payload["status"], "already_loaded");
    // Snapshot discipline: the loaded copy wins, untouched.
    assert_eq!(
        std::fs::read_to_string(dest.join("SKILL.md")).unwrap(),
        "pinned"
    );
}

#[test]
fn unknown_skill_declines_and_names_the_available_pool() {
    let repo = TempDir::new().unwrap();
    let home = TempDir::new().unwrap();
    seed_skill(home.path(), "bash");
    // A stray non-dir file in the pool is not an available skill.
    std::fs::write(home.path().join("skills/notes.txt"), b"x").unwrap();

    let err = run(
        &mut input("nope"),
        &mut Vec::new(),
        &env(repo.path(), "a1", home.path()),
    )
    .unwrap_err();
    let msg = err.to_string();
    assert!(matches!(err, Error::Unknown { .. }), "{msg}");
    assert!(msg.contains("bash"), "{msg}");
    assert!(!msg.contains("notes.txt"), "{msg}");
}

#[test]
fn unknown_skill_with_no_pool_reports_none_available() {
    let repo = TempDir::new().unwrap();
    let home = TempDir::new().unwrap(); // no skills/ dir at all
    let err = run(
        &mut input("nope"),
        &mut Vec::new(),
        &env(repo.path(), "a1", home.path()),
    )
    .unwrap_err();
    assert!(
        matches!(&err, Error::Unknown { available, .. } if available == "(none)"),
        "{err}"
    );
}

#[test]
fn bad_names_are_declined_without_touching_the_filesystem() {
    let repo = TempDir::new().unwrap();
    let home = TempDir::new().unwrap();
    for bad in ["", ".", "..", "a/b", "a\\b", "a\0b"] {
        let err = run(
            &mut input(bad),
            &mut Vec::new(),
            &env(repo.path(), "a1", home.path()),
        )
        .unwrap_err();
        assert!(matches!(err, Error::BadName(_)), "{bad:?} -> {err}");
    }
}

#[test]
fn invalid_json_surfaces_invalid_json() {
    let repo = TempDir::new().unwrap();
    let home = TempDir::new().unwrap();
    let err = run(
        &mut Cursor::new(b"not json".to_vec()),
        &mut Vec::new(),
        &env(repo.path(), "a1", home.path()),
    )
    .unwrap_err();
    assert!(matches!(err, Error::InvalidJson(_)), "{err}");
}

#[test]
fn missing_conv_repo_surfaces_missing_env() {
    let err = run(
        &mut input("git-ops"),
        &mut Vec::new(),
        &StubEnv(HashMap::new()),
    )
    .unwrap_err();
    assert!(
        matches!(err, Error::MissingEnv(k) if k == ENV_CONV_REPO),
        "{err}"
    );
}

#[test]
fn missing_conv_branch_surfaces_missing_env() {
    let repo = TempDir::new().unwrap();
    let mut m = HashMap::new();
    m.insert(ENV_CONV_REPO, repo.path().as_os_str().to_owned());
    let err = run(&mut input("git-ops"), &mut Vec::new(), &StubEnv(m)).unwrap_err();
    assert!(
        matches!(err, Error::MissingEnv(k) if k == ENV_CONV_BRANCH),
        "{err}"
    );
}

#[test]
fn non_utf8_conv_branch_surfaces_missing_env() {
    use std::os::unix::ffi::OsStringExt;
    let repo = TempDir::new().unwrap();
    let mut m = HashMap::new();
    m.insert(ENV_CONV_REPO, repo.path().as_os_str().to_owned());
    m.insert(ENV_CONV_BRANCH, OsString::from_vec(vec![0xff, 0xfe]));
    let err = run(&mut input("git-ops"), &mut Vec::new(), &StubEnv(m)).unwrap_err();
    assert!(
        matches!(err, Error::MissingEnv(k) if k == ENV_CONV_BRANCH),
        "{err}"
    );
}

#[test]
fn unresolvable_data_root_surfaces_root_error() {
    // Workspace + branch present, but no LITANY_HOME / XDG / HOME, so the
    // data-root resolution has nothing to stand on.
    let repo = TempDir::new().unwrap();
    let mut m = HashMap::new();
    m.insert(ENV_CONV_REPO, repo.path().as_os_str().to_owned());
    m.insert(ENV_CONV_BRANCH, OsString::from("a1"));
    let err = run(&mut input("git-ops"), &mut Vec::new(), &StubEnv(m)).unwrap_err();
    assert!(matches!(err, Error::Root(_)), "{err}");
}

#[test]
fn copy_failure_surfaces_copy_error() {
    let repo = TempDir::new().unwrap();
    let home = TempDir::new().unwrap();
    seed_skill(home.path(), "git-ops");
    // `skills` is a *file* where the copy needs a directory, so
    // `create_dir_all(dest)` fails.
    let skills = repo.path().join("agents/a1/skills");
    std::fs::create_dir_all(skills.parent().unwrap()).unwrap();
    std::fs::write(&skills, b"not a dir").unwrap();

    let err = run(
        &mut input("git-ops"),
        &mut Vec::new(),
        &env(repo.path(), "a1", home.path()),
    )
    .unwrap_err();
    assert!(matches!(err, Error::Copy { .. }), "{err}");
}

#[test]
fn stdin_read_error_surfaces_stdin_read() {
    struct Broken;
    impl Read for Broken {
        fn read(&mut self, _: &mut [u8]) -> io::Result<usize> {
            Err(io::Error::other("stdin broken"))
        }
    }
    let repo = TempDir::new().unwrap();
    let home = TempDir::new().unwrap();
    let err = run(
        &mut Broken,
        &mut Vec::new(),
        &env(repo.path(), "a1", home.path()),
    )
    .unwrap_err();
    assert!(matches!(err, Error::StdinRead(_)), "{err}");
}

#[test]
fn stdout_write_error_surfaces_write() {
    struct Broken;
    impl Write for Broken {
        fn write(&mut self, _: &[u8]) -> io::Result<usize> {
            Err(io::Error::other("stdout closed"))
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }
    let repo = TempDir::new().unwrap();
    let home = TempDir::new().unwrap();
    // Pre-existing copy → the `already_loaded` path reaches `emit`
    // without any filesystem copy, so the write error is what surfaces.
    std::fs::create_dir_all(repo.path().join("agents/a1/skills/git-ops")).unwrap();
    let err = run(
        &mut input("git-ops"),
        &mut Broken,
        &env(repo.path(), "a1", home.path()),
    )
    .unwrap_err();
    assert!(matches!(err, Error::Write(_)), "{err}");
}
