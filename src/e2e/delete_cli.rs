//! End-to-end subprocess tests for `litany delete` (ARCH §9.2
//! *Retention and GC*). The argv shape, the exit codes and the one-line
//! product are what a frontend spawns and parses (§3.5), so they are
//! pinned here against a real repo rather than only through the library
//! entry.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use tempfile::TempDir;

const INHERITED_GIT_ENV: &[&str] = &[
    "GIT_DIR",
    "GIT_WORK_TREE",
    "GIT_INDEX_FILE",
    "GIT_OBJECT_DIRECTORY",
    "GIT_PREFIX",
    "GIT_COMMON_DIR",
    "GIT_ALTERNATE_OBJECT_DIRECTORIES",
];

const ROOT: &str = "20260101-p1";
const CHILD: &str = "20260101-p1-20260102-c1";

fn git(dest: &Path, args: &[&str]) {
    let mut cmd = Command::new("git");
    for var in INHERITED_GIT_ENV {
        cmd.env_remove(var);
    }
    let out = cmd.arg("-C").arg(dest).args(args).output().expect("git");
    assert!(
        out.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

fn run(args: &[&str]) -> Output {
    let mut cmd = Command::new(crate::test_support::litany_binary());
    for var in INHERITED_GIT_ENV {
        cmd.env_remove(var);
    }
    cmd.args(args).output().expect("litany")
}

/// A workspace (§2.2) with a root agent, a child forked off its tip, and
/// both agents' worktrees, slices and a mark ref.
fn workspace() -> TempDir {
    let ws = TempDir::new().unwrap();
    let repo = ws.path().join("repo.git");
    std::fs::create_dir_all(&repo).unwrap();
    git(&repo, &["init", "-q", "--bare", "-b", "config/default"]);
    git(&repo, &["config", "user.email", "t@test.invalid"]);
    git(&repo, &["config", "user.name", "t"]);
    git(&repo, &["config", "core.hooksPath", "/dev/null"]);
    let author = ws.path().join(".author");
    let author_s = author.to_str().unwrap().to_owned();
    git(
        &repo,
        &[
            "worktree",
            "add",
            "-q",
            "--orphan",
            "-b",
            "config/default",
            &author_s,
        ],
    );
    std::fs::write(author.join("version"), "1\n").unwrap();
    git(&author, &["add", "-A"]);
    git(&author, &["commit", "-q", "-m", "config: init"]);
    git(&repo, &["worktree", "remove", &author_s]);

    let mut from = repo.clone();
    for (id, start) in [(ROOT, "config/default"), (CHILD, "HEAD")] {
        let wt = ws.path().join("agents").join(id);
        let wt_s = wt.to_str().unwrap().to_owned();
        git(
            &from,
            &[
                "worktree",
                "add",
                "-q",
                &wt_s,
                "-b",
                &format!("agents/{id}"),
                start,
            ],
        );
        std::fs::write(wt.join("goal.md"), id).unwrap();
        git(&wt, &["add", "-A"]);
        git(&wt, &["commit", "-q", "-m", "dispatch"]);
        std::fs::create_dir_all(ws.path().join("steps").join(id).join("001")).unwrap();
        std::fs::write(ws.path().join("steps").join(id).join("001/meta.json"), "{}").unwrap();
        std::fs::create_dir_all(ws.path().join("inbox").join(id)).unwrap();
        std::fs::write(ws.path().join("inbox").join(id).join("user-001.md"), "hi").unwrap();
        git(
            &repo,
            &[
                "update-ref",
                &format!("refs/litany/notify/{id}"),
                &format!("refs/heads/agents/{id}"),
            ],
        );
        from = wt;
    }
    ws
}

fn refs(ws: &Path) -> String {
    // Scrubbed like every other git spawn here: under a git hook (the
    // pre-commit gate is one) `GIT_DIR` is set and overrides `-C`, so an
    // unscrubbed listing reports the *outer* repo's refs and every
    // assertion over this string reads the wrong repository.
    let mut cmd = Command::new("git");
    for var in INHERITED_GIT_ENV {
        cmd.env_remove(var);
    }
    let out = cmd
        .arg("-C")
        .arg(ws.join("repo.git"))
        .args(["for-each-ref", "--format=%(refname)"])
        .output()
        .expect("git");
    // Assert the enumeration itself: an unchecked failure here reads as
    // "the ref is gone" at every call site, which is the opposite of
    // what happened.
    assert!(
        out.status.success(),
        "git for-each-ref: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).into_owned()
}

fn ws_arg(ws: &TempDir) -> PathBuf {
    ws.path().to_path_buf()
}

#[test]
fn the_bare_form_refuses_a_subtree_dry_run_plans_it_and_children_takes_it() {
    let ws = workspace();
    let path = ws_arg(&ws);
    let p = path.to_str().unwrap();

    // Bare: declined, naming the descendant and the flag that would
    // include it. Exit non-zero, nothing on stdout (§3.4).
    let out = run(&["delete", p, ROOT]);
    assert!(!out.status.success());
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(err.starts_with("litany delete: "), "{err}");
    assert!(err.contains(CHILD) && err.contains("--children"), "{err}");
    assert!(out.stdout.is_empty());
    assert!(refs(ws.path()).contains(&format!("refs/heads/agents/{ROOT}")));

    // The plan: the census on stdout, the workspace untouched.
    let out = run(&["delete", p, ROOT, "--children", "--dry-run"]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(
        String::from_utf8_lossy(&out.stdout).trim(),
        format!("would delete {ROOT}; descendants: 1 ({CHILD}); pending deposits: 2")
    );
    assert!(ws.path().join("agents").join(CHILD).exists());

    // The act: same sentence, past tense, and every slice is gone.
    let out = run(&["delete", p, ROOT, "--children"]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(
        String::from_utf8_lossy(&out.stdout).trim(),
        format!("deleted {ROOT}; descendants: 1 ({CHILD}); pending deposits: 2")
    );
    let after = refs(ws.path());
    assert!(!after.contains("agents/"), "{after}");
    assert!(!after.contains("refs/litany/"), "{after}");
    for id in [ROOT, CHILD] {
        for dir in ["agents", "steps", "inbox"] {
            assert!(
                !ws.path().join(dir).join(id).exists(),
                "{dir}/{id} survived"
            );
        }
    }
    // Convergent: a second run over the absent subtree is a quiet success.
    let out = run(&["delete", p, ROOT, "--children"]);
    assert!(out.status.success());
    assert_eq!(
        String::from_utf8_lossy(&out.stdout).trim(),
        format!("deleted {ROOT}; descendants: 0; pending deposits: 0")
    );
}
