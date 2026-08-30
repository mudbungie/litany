//! Branch-state lookup for `litany stop`.
//!
//! One question needs answering before sending any signal: does the
//! agent branch `agents/<id>` exist in the workspace repository? An
//! exit-code question on `git`, so the trait is shaped around it rather
//! than a generic "run-git" surface. Tests inject a stub that returns
//! the bit directly; production shells out via the supplied
//! [`GitRunner`] against `<workspace>/repo.git` (ARCH §2.2 — the bare
//! workspace repository).
//!
//! The v0.3-era "already merged into main" refusal is gone with `main`
//! itself (§2.2–§2.3): no branch merges anywhere, so there is no merged
//! state to refuse — an already-terminal branch is simply a stop with
//! no lock holder, idempotently `Ok(())`.

use crate::template::GitRunner;
use std::io;
use std::path::Path;

/// The ref-existence question [`super::run`] needs answered before
/// signaling. The trait is `&dyn`-shaped so tests pass a stub and
/// production passes [`GitInspector`] without paying the subprocess
/// cost in the test path.
pub trait BranchInspector {
    fn exists(&self, repo: &Path, branch: &str, git: &dyn GitRunner) -> io::Result<bool>;
}

/// Production [`BranchInspector`] — runs `git rev-parse --verify
/// refs/heads/agents/<branch>` against `<workspace>/repo.git`.
#[derive(Debug, Default, Clone, Copy)]
pub struct GitInspector;

impl BranchInspector for GitInspector {
    fn exists(&self, repo: &Path, branch: &str, git: &dyn GitRunner) -> io::Result<bool> {
        // The question has one home ([`crate::workspace::agent_exists`]);
        // this impl is only the trait seam over it.
        Ok(crate::workspace::agent_exists(repo, branch, git))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;
    use std::path::PathBuf;

    struct RecordingGit {
        invocations: RefCell<Vec<(PathBuf, Vec<String>)>>,
        result: io::Result<()>,
    }

    impl GitRunner for RecordingGit {
        fn run(&self, dest: &Path, args: &[&str]) -> io::Result<()> {
            self.invocations.borrow_mut().push((
                dest.to_path_buf(),
                args.iter().map(|s| (*s).to_owned()).collect(),
            ));
            // `io::Error` isn't `Clone`, so we mirror the kind /
            // message rather than re-emit the same instance.
            match &self.result {
                Ok(()) => Ok(()),
                Err(e) => Err(io::Error::new(e.kind(), e.to_string())),
            }
        }
        fn run_capture(&self, _: &Path, _: &[&str]) -> io::Result<String> {
            unreachable!("inspector never calls run_capture")
        }
    }

    fn ok_git() -> RecordingGit {
        RecordingGit {
            invocations: RefCell::new(Vec::new()),
            result: Ok(()),
        }
    }
    fn err_git() -> RecordingGit {
        RecordingGit {
            invocations: RefCell::new(Vec::new()),
            result: Err(io::Error::other("boom")),
        }
    }

    #[test]
    fn exists_true_on_zero_exit_and_probes_the_agents_ref_in_repo_git() {
        let git = ok_git();
        assert!(
            GitInspector
                .exists(&PathBuf::from("/w"), "br", &git)
                .unwrap()
        );
        let invocations = git.invocations.borrow();
        // The §8 ref namespace: the id maps to `agents/<id>` at the git
        // boundary, and the question is asked of the bare repo.git.
        assert_eq!(invocations[0].0, PathBuf::from("/w/repo.git"));
        assert_eq!(
            invocations[0].1,
            vec!["rev-parse", "--verify", "--quiet", "refs/heads/agents/br"]
        );
    }

    #[test]
    fn exists_false_on_nonzero_exit() {
        let git = err_git();
        assert!(
            !GitInspector
                .exists(&PathBuf::from("/w"), "br", &git)
                .unwrap()
        );
    }
}
