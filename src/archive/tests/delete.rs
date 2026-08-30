//! `delete` cases (ARCH §9.2 *Retention and GC*) against **real git** in
//! a real workspace fixture: the removal covers every slice of an agent,
//! the two refusals fail closed, `--dry-run` writes nothing, and a
//! half-finished delete is completed by a re-run.
//!
//! Real git rather than the [`super::StubGit`] because what is under test
//! *is* the git-and-filesystem effect — a stub would assert the argv this
//! module already reads off its own source. The stub returns for the two
//! failure arms no real repo produces on demand.

use super::super::delete;
use crate::prompt::inbox::{inbox_dir, try_acquire};
use crate::template::{GitRunner, RealGit};
use crate::workspace::{self, agent_ref, agent_worktree, repo_git};
use std::io;
use std::path::{Path, PathBuf};

const ROOT: &str = "20260101-a1";
const CHILD: &str = "20260101-a1-20260102-b2";

/// A workspace with a root agent, its child, and both agents' full
/// complement of state: worktree, `steps/`, `inbox/`, and a mark ref.
fn tree() -> (tempfile::TempDir, PathBuf) {
    let (holder, ws) = workspace::fixture::workspace();
    workspace::fixture::spawn_root(&ws, ROOT);
    workspace::fixture::spawn_agent(&ws, CHILD, &agent_ref(ROOT));
    for id in [ROOT, CHILD] {
        super::write(&ws.join("steps").join(id).join("001/meta.json"), "{}");
        super::write(&ws.join("inbox").join(id).join("user-001.md"), "hi");
        RealGit::new()
            .run(
                &repo_git(&ws),
                &[
                    "update-ref",
                    &format!("refs/litany/notify/{id}"),
                    &format!("refs/heads/{}", agent_ref(id)),
                ],
            )
            .unwrap();
    }
    (holder, ws)
}

/// Every home an id has (§2.2, §2.3, §2.11): ref, worktree, both slices,
/// and the mark. `true` iff *nothing* remembers it.
fn gone(ws: &Path, id: &str) -> bool {
    let git = RealGit::new();
    let marks = git
        .run_capture(
            &repo_git(ws),
            &["for-each-ref", "--format=%(refname)", "refs/litany/"],
        )
        .unwrap();
    !workspace::agent_exists(ws, id, &git)
        && !agent_worktree(ws, id).exists()
        && !ws.join("steps").join(id).exists()
        && !ws.join("inbox").join(id).exists()
        && !marks.contains(id)
}

#[test]
fn a_leaf_delete_removes_every_slice_and_reports_what_died() {
    let (_h, ws) = tree();
    // The leaf: delete the child, bare form.
    let report = delete(&ws, CHILD, false, false, &RealGit::new()).unwrap();
    assert_eq!(report.descendants, Vec::<String>::new());
    assert_eq!(report.pending_deposits, 1);
    assert_eq!(
        report.to_string(),
        format!("deleted {CHILD}; descendants: 0; pending deposits: 1")
    );
    assert!(gone(&ws, CHILD));
    // The parent is untouched — a delete cuts the subtree, nothing above it.
    assert!(!gone(&ws, ROOT));
}

#[test]
fn the_bare_form_declines_a_subtree_and_names_the_descendants() {
    let (_h, ws) = tree();
    let err = delete(&ws, ROOT, false, false, &RealGit::new()).unwrap_err();
    let msg = err.to_string();
    assert!(
        matches!(err, delete::DeleteError::HasDescendants { .. }),
        "{err:?}"
    );
    assert!(msg.contains(CHILD), "{msg}");
    assert!(msg.contains("--children"), "{msg}");
    // Fail closed: nothing was removed.
    assert!(!gone(&ws, ROOT) && !gone(&ws, CHILD));
}

#[test]
fn children_removes_the_whole_subtree() {
    let (_h, ws) = tree();
    let report = delete(&ws, ROOT, true, false, &RealGit::new()).unwrap();
    assert_eq!(report.descendants, vec![CHILD.to_string()]);
    assert_eq!(report.pending_deposits, 2);
    assert!(
        report
            .to_string()
            .starts_with(&format!("deleted {ROOT}; descendants: 1 ({CHILD})"))
    );
    assert!(gone(&ws, ROOT) && gone(&ws, CHILD));
}

#[test]
fn a_dry_run_is_the_same_census_and_removes_nothing() {
    let (_h, ws) = tree();
    let plan = delete(&ws, ROOT, true, true, &RealGit::new()).unwrap();
    assert!(!plan.removed);
    assert_eq!(
        plan.to_string(),
        format!("would delete {ROOT}; descendants: 1 ({CHILD}); pending deposits: 2")
    );
    assert!(!gone(&ws, ROOT) && !gone(&ws, CHILD));
    // …and the real run that follows says the same thing in the past tense.
    let done = delete(&ws, ROOT, true, false, &RealGit::new()).unwrap();
    assert_eq!(done.descendants, plan.descendants);
    assert_eq!(done.pending_deposits, plan.pending_deposits);
}

#[test]
fn a_re_run_completes_a_partial_delete_and_then_finds_nothing() {
    let (_h, ws) = tree();
    let git = RealGit::new();
    // A delete that died after dropping the ref: the worktree, both
    // slices and the mark survive it (§9.2 — the target set is the union
    // of the id's homes, so the leftovers are still enumerated).
    git.run(
        &repo_git(&ws),
        &[
            "update-ref",
            "-d",
            &format!("refs/heads/{}", agent_ref(CHILD)),
        ],
    )
    .unwrap();
    // …and got as far as the two slices before it died, leaving only the
    // worktree and the mark. Every absent home is simply an empty input.
    std::fs::remove_dir_all(ws.join("steps").join(CHILD)).unwrap();
    std::fs::remove_dir_all(ws.join("inbox").join(CHILD)).unwrap();
    let report = delete(&ws, CHILD, false, false, &git).unwrap();
    assert_eq!(report.pending_deposits, 0);
    assert!(gone(&ws, CHILD));
    // Convergent: a third run over an agent nothing remembers is a quiet
    // success with an empty census, not an error.
    let again = delete(&ws, CHILD, false, false, &git).unwrap();
    assert_eq!(
        again.to_string(),
        format!("deleted {CHILD}; descendants: 0; pending deposits: 0")
    );
}

#[test]
fn a_driven_agent_is_declined_naming_its_lock() {
    let (_h, ws) = tree();
    // A live driver's lease on the child's inbox (§2.11). Two open file
    // descriptions on one directory contend even inside one process, so
    // this is the real refusal, not a simulation of it.
    let lease = try_acquire(&inbox_dir(&ws, CHILD)).unwrap().unwrap();
    let err = delete(&ws, ROOT, true, false, &RealGit::new()).unwrap_err();
    let msg = err.to_string();
    assert!(matches!(err, delete::DeleteError::Driven { .. }), "{err:?}");
    assert!(msg.contains(CHILD) && msg.contains("§2.11"), "{msg}");
    assert!(
        msg.contains(&inbox_dir(&ws, CHILD).display().to_string()),
        "{msg}"
    );
    // Nothing was reaped beneath the driver — not even the quiescent root.
    assert!(!gone(&ws, ROOT) && !gone(&ws, CHILD));
    drop(lease);
    // Quiescent again, the same call goes through.
    delete(&ws, ROOT, true, false, &RealGit::new()).unwrap();
    assert!(gone(&ws, ROOT) && gone(&ws, CHILD));
}

#[test]
fn a_lock_home_that_is_not_a_directory_is_declined_not_assumed_quiescent() {
    let (_h, ws) = tree();
    // Debris where the child's inbox belongs: the probe cannot open its
    // home, so the delete declines rather than reading the failure as
    // "nobody is driving" (§2.11 — liveness is observed, never assumed).
    std::fs::remove_dir_all(inbox_dir(&ws, CHILD)).unwrap();
    std::fs::write(inbox_dir(&ws, CHILD), b"not a dir").unwrap();
    let err = delete(&ws, CHILD, false, false, &RealGit::new()).unwrap_err();
    assert!(matches!(err, delete::DeleteError::Probe { .. }), "{err:?}");
    assert!(!gone(&ws, CHILD));
}

#[test]
fn bundle_composes_in_front_of_delete() {
    let (_h, ws) = tree();
    let out = super::tmp();
    // Archive-then-delete is two verbs the caller sequences (§9.2): the
    // archive is complete before the workspace state goes, and delete
    // never touches the archive.
    super::super::bundle(&ws, ROOT, out.path(), &RealGit::new()).unwrap();
    delete(&ws, ROOT, true, false, &RealGit::new()).unwrap();
    assert!(out.path().join(super::super::BUNDLE_FILE).exists());
    assert!(
        out.path()
            .join("steps")
            .join(ROOT)
            .join("001/meta.json")
            .exists()
    );
    assert!(
        out.path()
            .join("inbox")
            .join(CHILD)
            .join("user-001.md")
            .exists()
    );
    assert!(gone(&ws, ROOT) && gone(&ws, CHILD));
}

#[test]
fn the_layout_guard_declines_a_non_workspace() {
    let dir = super::tmp();
    let err = delete(dir.path(), ROOT, false, false, &RealGit::new()).unwrap_err();
    assert!(matches!(err, delete::DeleteError::Layout(_)), "{err:?}");
}

#[test]
fn git_failures_surface_with_the_op_that_failed() {
    let ws = super::ws_tmp();
    // The mark enumeration is the first git op; failing every capture
    // stops there.
    let err = delete(
        ws.path(),
        ROOT,
        false,
        false,
        &super::StubGit::new("").fail_capture(),
    )
    .unwrap_err();
    assert!(
        matches!(err, delete::DeleteError::Git { op, .. } if op == "for-each-ref refs/litany/"),
        "{err:?}"
    );
    // The subtree enumeration is the second: a stub that answers
    // `for-each-ref` and fails `branch --list`.
    let err = delete(ws.path(), ROOT, false, false, &MarksOnly(true)).unwrap_err();
    assert!(
        matches!(err, delete::DeleteError::Git { op, .. } if op == "branch --list"),
        "{err:?}"
    );
    // And the removal's own ops: `worktree prune` runs first, so a stub
    // whose every `run` fails reports it.
    let err = delete(ws.path(), ROOT, false, false, &MarksOnly(false)).unwrap_err();
    assert!(
        matches!(err, delete::DeleteError::Git { op, .. } if op == "worktree prune"),
        "{err:?}"
    );
}

/// A `GitRunner` that answers the mark enumeration with nothing and then
/// fails: on `branch --list` when `true`, on every `run` when `false`.
struct MarksOnly(bool);

impl GitRunner for MarksOnly {
    fn run(&self, _dest: &Path, _args: &[&str]) -> io::Result<()> {
        Err(io::Error::other("no runs here"))
    }
    fn run_capture(&self, _dest: &Path, args: &[&str]) -> io::Result<String> {
        match args.first().copied() {
            Some("for-each-ref") => Ok(String::new()),
            _ if self.0 => Err(io::Error::other("branch --list fails")),
            _ => Ok(String::new()),
        }
    }
}

#[test]
fn error_messages_render() {
    let cases: Vec<delete::DeleteError> = vec![
        delete::DeleteError::Layout(workspace::LayoutError::NotAWorkspace(PathBuf::from("/x"))),
        delete::DeleteError::Io(io::Error::other("x")),
        delete::DeleteError::Git {
            op: "update-ref -d",
            source: io::Error::other("x"),
        },
        delete::DeleteError::HasDescendants {
            id: ROOT.into(),
            descendants: vec![CHILD.into()],
        },
        delete::DeleteError::Driven {
            id: ROOT.into(),
            lock: PathBuf::from("/i"),
        },
        delete::DeleteError::Probe {
            path: PathBuf::from("/i"),
            source: io::Error::other("x"),
        },
    ];
    for e in cases {
        assert!(!format!("{e}").is_empty());
    }
}
