//! Unit tests for caller-supplied pinned documents: destination
//! validation, spec parsing/loading, collision refusal, and the write
//! path the dispatch commits stage from.

use super::{PinError, PinnedDoc, PinnedDocs, load};

fn doc(dest: &str) -> Result<PinnedDoc, PinError> {
    PinnedDoc::new(dest.to_owned(), b"bytes".to_vec())
}

fn dest_rule(dest: &str) -> String {
    match doc(dest).unwrap_err() {
        PinError::Dest { rule, .. } => rule,
        other => panic!("expected Dest refusal, got {other}"),
    }
}

#[test]
fn accepts_plain_and_nested_destinations() {
    for ok in ["AGENTS.md", "docs/notes.md", "a/b/c.txt", "skills/x.md"] {
        let d = doc(ok).unwrap();
        assert_eq!(d.dest(), ok);
        assert_eq!(d.bytes, b"bytes");
    }
}

#[test]
fn refuses_empty_absolute_and_traversal_destinations() {
    assert_eq!(dest_rule(""), "is empty");
    assert_eq!(
        dest_rule("/etc/passwd"),
        "must be relative to the worktree root"
    );
    for bad in ["../up.md", "a/../b.md", "./x.md", "a/./b", "a//b", "a/"] {
        assert_eq!(
            dest_rule(bad),
            "may not contain empty, '.' or '..' segments",
            "{bad}"
        );
    }
}

#[test]
fn refuses_git_segments_case_insensitively() {
    for bad in [".git/config", "a/.GIT/hook", "b/.Git"] {
        assert_eq!(dest_rule(bad), "may not enter .git", "{bad}");
    }
}

#[test]
fn refuses_every_harness_owned_first_segment() {
    // The system slot, the lineage's facts file (§5.5 — the dispatch
    // commit cuts it out of the governing config commit, so a pin there
    // would be overwritten by the very commit it rode in on), the
    // control files the trim removes (§2.2), and the harness-derived
    // trees: pins may not inject into any of them.
    for reserved in [
        "goal.md",
        "soul.md",
        "name",
        "facts.md",
        "manifest.yaml",
        "workflow.yaml",
        "providers.yaml",
        "version",
        "souls",
        "descriptions",
        "messages",
        "summary",
    ] {
        let file = dest_rule(reserved);
        assert!(file.contains("harness-owned"), "{reserved}: {file}");
        let nested = dest_rule(&format!("{reserved}/inner.md"));
        assert!(nested.contains("harness-owned"), "{reserved}: {nested}");
    }
    // ...but a deeper segment of the same name is ordinary context.
    doc("docs/messages/log.md").unwrap();
}

#[test]
fn refuses_colliding_destinations() {
    let err = PinnedDocs::new(vec![doc("a.md").unwrap(), doc("a.md").unwrap()]).unwrap_err();
    assert!(matches!(err, PinError::Collision { dest } if dest == "a.md"));
}

#[test]
fn load_reads_exact_bytes_split_at_the_first_equals() {
    let dir = tempfile::TempDir::new().unwrap();
    let src = dir.path().join("src=file.md");
    std::fs::write(&src, b"exact\x00bytes").unwrap();
    let pins = load(&[format!("docs/pinned.md={}", src.display())]).unwrap();
    let got: Vec<_> = pins.iter().collect();
    assert_eq!(got.len(), 1);
    assert_eq!(got[0].dest(), "docs/pinned.md");
    assert_eq!(got[0].bytes, b"exact\x00bytes");
}

#[test]
fn load_refuses_malformed_specs() {
    for spec in ["no-equals", "=src", "dest="] {
        let err = load(&[spec.to_owned()]).unwrap_err();
        assert!(
            matches!(&err, PinError::Spec { spec: s } if s == spec),
            "{spec}: {err}"
        );
        assert!(err.to_string().contains("<dest>=<source-path>"));
    }
}

#[test]
fn load_refuses_an_unreadable_source() {
    let err = load(&["a.md=/no/such/source".to_owned()]).unwrap_err();
    match err {
        PinError::Source { path, .. } => {
            assert_eq!(path, std::path::PathBuf::from("/no/such/source"));
        }
        other => panic!("expected Source refusal, got {other}"),
    }
}

#[test]
fn write_into_lands_every_document_creating_directories() {
    let dir = tempfile::TempDir::new().unwrap();
    let pins = PinnedDocs::new(vec![
        PinnedDoc::new("top.md".into(), b"t".to_vec()).unwrap(),
        PinnedDoc::new("deep/nested/doc.md".into(), b"d".to_vec()).unwrap(),
    ])
    .unwrap();
    pins.write_into(dir.path()).unwrap();
    assert_eq!(std::fs::read(dir.path().join("top.md")).unwrap(), b"t");
    assert_eq!(
        std::fs::read(dir.path().join("deep/nested/doc.md")).unwrap(),
        b"d"
    );
}

#[test]
fn none_is_the_empty_set() {
    let none = PinnedDocs::none();
    assert_eq!(none.iter().count(), 0);
    let dir = tempfile::TempDir::new().unwrap();
    none.write_into(dir.path()).unwrap();
    assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 0);
}

#[test]
fn write_into_refuses_a_symlinked_directory_component() {
    // The inherited tree carries `docs -> <outside>`: writing
    // `docs/x.md` through it would land the bytes outside the worktree.
    let dir = tempfile::TempDir::new().unwrap();
    let outside = tempfile::TempDir::new().unwrap();
    std::os::unix::fs::symlink(outside.path(), dir.path().join("docs")).unwrap();
    let pins = PinnedDocs::new(vec![
        PinnedDoc::new("docs/x.md".into(), b"pinned".to_vec()).unwrap(),
    ])
    .unwrap();
    let err = pins.write_into(dir.path()).unwrap_err();
    assert!(err.to_string().contains("symlink"), "{err}");
    assert!(err.to_string().contains("docs"), "{err}");
    assert!(!outside.path().join("x.md").exists(), "the write escaped");
}

#[test]
fn write_into_refuses_a_symlinked_final_path() {
    // `evil.md -> <outside file>`: writing through it would silently
    // overwrite the target while the commit snapshots only the symlink.
    let dir = tempfile::TempDir::new().unwrap();
    let outside = tempfile::TempDir::new().unwrap();
    let target = outside.path().join("target.md");
    std::fs::write(&target, b"original").unwrap();
    std::os::unix::fs::symlink(&target, dir.path().join("evil.md")).unwrap();
    let pins = PinnedDocs::new(vec![
        PinnedDoc::new("evil.md".into(), b"pinned".to_vec()).unwrap(),
    ])
    .unwrap();
    let err = pins.write_into(dir.path()).unwrap_err();
    assert!(err.to_string().contains("symlink"), "{err}");
    assert_eq!(std::fs::read(&target).unwrap(), b"original");
}
