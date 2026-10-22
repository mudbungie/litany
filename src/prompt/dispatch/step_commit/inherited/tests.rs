//! Unit tests for the dispatch-time inherited-dialog prune (ARCH §2.2,
//! §2.5). The fork-level proof — a real child dispatched off a parent
//! whose last settled instruction was itself "dispatch a child", whose
//! first assembled context carries none of that dialog — is [`fork`].
//!
//! Lives in a sibling file rather than an inline `mod tests` so the
//! production module stays under the 300-line repo cap.

mod fork;

use super::*;
use std::cell::RefCell;
use std::io;
use std::path::PathBuf;

#[derive(Default)]
struct StubGit {
    runs: RefCell<Vec<(PathBuf, Vec<String>)>>,
    fail: bool,
}

impl GitRunner for StubGit {
    fn run(&self, dest: &Path, args: &[&str]) -> io::Result<()> {
        self.runs.borrow_mut().push((
            dest.to_path_buf(),
            args.iter().map(|s| (*s).to_owned()).collect(),
        ));
        if self.fail {
            Err(io::Error::other("stub git fail"))
        } else {
            Ok(())
        }
    }
    fn run_capture(&self, _dest: &Path, _args: &[&str]) -> io::Result<String> {
        unreachable!("the prune never issues capturing git ops")
    }
}

#[test]
fn a_worker_child_stages_the_removal_of_every_dialog_path() {
    let git = StubGit::default();
    prune_inherited_dialog(Path::new("/wt"), false, &git).unwrap();
    let runs = git.runs.borrow();
    assert_eq!(runs.len(), 1);
    assert_eq!(runs[0].0, PathBuf::from("/wt"));
    assert_eq!(
        runs[0].1,
        [
            "rm",
            "-r",
            "-q",
            "--ignore-unmatch",
            "--",
            "messages",
            "summary",
            "skills"
        ]
        .map(String::from)
    );
}

#[test]
fn a_keeper_keeps_its_subject_and_issues_no_git_op() {
    // §2.7: the dispatching branch's transcript, summary chain and
    // spent skill bodies are a procedure child's input — the prune
    // skips it. Who keeps is the caller's derivation (machinery under
    // the governing workflow); here the answer is handed in.
    let git = StubGit::default();
    prune_inherited_dialog(Path::new("/wt"), true, &git).unwrap();
    assert!(git.runs.borrow().is_empty());
}

#[test]
fn a_git_failure_surfaces_as_the_named_op() {
    let git = StubGit {
        fail: true,
        ..StubGit::default()
    };
    let err = prune_inherited_dialog(Path::new("/wt"), false, &git).unwrap_err();
    assert!(
        matches!(&err, Error::Git { op, .. } if *op == "rm inherited dialog"),
        "got {err:?}"
    );
}
