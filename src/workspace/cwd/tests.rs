//! The working-directory mark against a real workspace (ARCH §3.3).

use super::{cwd_ref, read, resolve, write};
use crate::template::{GitRunner, RealGit};
use crate::workspace::{fixture, repo_git};
use std::path::{Path, PathBuf};

/// A workspace with one root agent — the shape every tool call runs in.
fn agent() -> (tempfile::TempDir, PathBuf) {
    let (holder, ws) = fixture::workspace();
    fixture::spawn_root(&ws, "a");
    (holder, ws)
}

#[test]
fn the_mark_ref_lives_in_the_shared_per_agent_mark_namespace() {
    // §9.2's retention delete enumerates `refs/litany/`, so a mark that
    // spells its own root would outlive the agent it belongs to.
    assert_eq!(cwd_ref("a-b"), "refs/litany/cwd/a-b");
}

#[test]
fn an_unset_mark_reads_as_none_so_the_default_applies() {
    let (_h, ws) = agent();
    assert_eq!(read(&ws, "a", &RealGit::new()), None);
}

#[test]
fn a_written_mark_reads_back_the_same_absolute_path() {
    let (_h, ws) = agent();
    let git = RealGit::new();
    let dir = ws.join("agents/a");
    write(&ws, "a", &dir, &git).unwrap();
    assert_eq!(read(&ws, "a", &git), Some(dir));
}

#[test]
fn a_second_write_wins_because_a_cd_replaces_where_the_agent_is() {
    let (_h, ws) = agent();
    let git = RealGit::new();
    write(&ws, "a", Path::new("/tmp"), &git).unwrap();
    write(&ws, "a", Path::new("/usr"), &git).unwrap();
    assert_eq!(read(&ws, "a", &git), Some(PathBuf::from("/usr")));
}

#[test]
fn the_mark_is_per_agent_so_one_agents_cd_never_moves_another() {
    let (_h, ws) = agent();
    let git = RealGit::new();
    fixture::spawn_root(&ws, "b");
    write(&ws, "a", Path::new("/tmp"), &git).unwrap();
    assert_eq!(read(&ws, "b", &git), None);
}

#[test]
fn hashing_the_value_leaves_nothing_behind_beside_the_repo() {
    let (_h, ws) = agent();
    write(&ws, "a", Path::new("/tmp"), &RealGit::new()).unwrap();
    let leftovers: Vec<_> = std::fs::read_dir(repo_git(&ws))
        .unwrap()
        .flatten()
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| n.starts_with("cwd-mark."))
        .collect();
    assert!(
        leftovers.is_empty(),
        "staged value left behind: {leftovers:?}"
    );
}

#[test]
fn a_mark_pointing_at_a_non_blob_reads_as_none_rather_than_failing_a_tool_call() {
    let (_h, ws) = agent();
    let git = RealGit::new();
    let repo = repo_git(&ws);
    let head = git.run_capture(&repo, &["rev-parse", "agents/a"]).unwrap();
    git.run(&repo, &["update-ref", &cwd_ref("a"), &head])
        .unwrap();
    assert_eq!(read(&ws, "a", &git), None);
}

#[test]
fn a_workspace_with_no_repo_reads_as_none() {
    let holder = tempfile::TempDir::new().unwrap();
    assert_eq!(read(holder.path(), "a", &RealGit::new()), None);
}

#[test]
fn a_path_that_trimming_would_change_is_declined_rather_than_stored_wrong() {
    let (_h, ws) = agent();
    let err = write(&ws, "a", Path::new("/tmp/trailing "), &RealGit::new()).unwrap_err();
    assert!(err.to_string().contains("trimmed UTF-8"), "{err}");
    assert_eq!(read(&ws, "a", &RealGit::new()), None);
}

#[test]
fn an_empty_path_is_declined() {
    let (_h, ws) = agent();
    let err = write(&ws, "a", Path::new(""), &RealGit::new()).unwrap_err();
    assert!(err.to_string().contains("trimmed UTF-8"), "{err}");
}

#[test]
fn a_non_utf8_path_is_declined() {
    use std::ffi::OsStr;
    use std::os::unix::ffi::OsStrExt;
    let (_h, ws) = agent();
    let bad = PathBuf::from(OsStr::from_bytes(b"/tmp/\xff"));
    let err = write(&ws, "a", &bad, &RealGit::new()).unwrap_err();
    assert!(err.to_string().contains("trimmed UTF-8"), "{err}");
}

#[test]
fn a_git_that_cannot_hash_the_value_surfaces_the_failure() {
    let (_h, ws) = agent();
    // No `repo.git` under this path, so `hash-object` has no object
    // database to write into.
    let bare = ws.join("nowhere");
    std::fs::create_dir_all(super::repo_git(&bare)).unwrap();
    let err = write(&bare, "a", Path::new("/tmp"), &RealGit::new()).unwrap_err();
    assert!(err.to_string().contains("hash-object"), "{err}");
}

#[test]
fn a_value_that_cannot_be_staged_surfaces_the_io_failure() {
    let holder = tempfile::TempDir::new().unwrap();
    // `repo.git` absent entirely: staging the value has nowhere to land.
    let err = write(holder.path(), "a", Path::new("/tmp"), &RealGit::new()).unwrap_err();
    assert_eq!(err.kind(), std::io::ErrorKind::NotFound);
}

/// [`resolve`] — the one validation both writers of the mark run: the
/// `cd` built-in mid-run and `--cwd` at agent creation (§3.3). Its
/// answers are the kernel's, so a directory is a directory in one voice.
mod resolving {
    use super::*;

    #[test]
    fn an_existing_directory_resolves_to_its_absolute_self() {
        let holder = tempfile::TempDir::new().unwrap();
        let expected = std::fs::canonicalize(holder.path()).unwrap();
        assert_eq!(resolve(holder.path()).unwrap(), expected);
    }

    #[test]
    fn dot_dot_and_symlinks_are_resolved_not_stored_literally() {
        let holder = tempfile::TempDir::new().unwrap();
        std::fs::create_dir(holder.path().join("inner")).unwrap();
        let resolved = resolve(&holder.path().join("inner/..")).unwrap();
        assert!(!resolved.to_string_lossy().contains(".."), "{resolved:?}");
        assert_eq!(resolved, std::fs::canonicalize(holder.path()).unwrap());
    }

    #[test]
    fn a_path_that_names_nothing_is_declined() {
        let err = resolve(Path::new("/no/such/place/at/all")).unwrap_err();
        assert!(err.to_string().contains("no such directory"), "{err}");
    }

    #[test]
    fn a_file_is_declined_because_it_is_not_a_directory() {
        let holder = tempfile::TempDir::new().unwrap();
        let file = holder.path().join("f");
        std::fs::write(&file, b"x").unwrap();
        let err = resolve(&file).unwrap_err();
        assert!(err.to_string().contains("is not a directory"), "{err}");
    }

    #[test]
    fn a_directory_the_mark_could_not_store_is_declined_before_it_is_written() {
        // Trailing whitespace survives the filesystem but not the mark's
        // trimmed-UTF-8 round trip, so the refusal belongs here — at the
        // caller's path, before any agent exists.
        let holder = tempfile::TempDir::new().unwrap();
        let trailing = holder.path().join("dir ");
        std::fs::create_dir(&trailing).unwrap();
        let err = resolve(&trailing).unwrap_err();
        assert!(err.to_string().contains("trimmed UTF-8"), "{err}");
    }
}
