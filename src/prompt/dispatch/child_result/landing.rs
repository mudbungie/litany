//! The `land_compaction` action of the §6 binding interpreter (ARCH
//! §2.6, §2.7) — the compaction landing as the child-result interpreter
//! reaches it, beside [`super::flush`] (which *starts* a compaction pass)
//! and [`super::verifier`] (which owns the gate).
//!
//! Three facts live here and nowhere else: which return qualifies to
//! land ([`qualifies`]), what a compaction landing that did not simply
//! succeed says to the operator ([`land`]), and the **generic landing**
//! ([`land_response`], `docs/DESIGN_PROCEDURES.md` §4) — a child's
//! terminal response committed as one file at the dispatcher's branch
//! root, with no transfer and no transcript entry. The interpreter above
//! decides only *whether* an action fires.

use super::ChildResult;
use crate::config::Workflow;
use crate::prompt::notice::notice;
use crate::prompt::{Error, compactor, inbox};
use crate::template::GitRunner;
use std::path::Path;

/// Does this return qualify for a **landing** — the compaction landing
/// here, the reviewer's [`super::proposal::stage`], or the generic
/// [`land_response`]? Only a
/// `final-response` epitaph does (§2.6/§2.7 — a child that ends on any
/// other epitaph lands nothing): the epitaph is the pinned manner of
/// ending, and code branches on its value (§2.6). One gate for both,
/// because it is one question: did the pass this child was forked to
/// perform actually finish?
pub(super) fn qualifies(cr: &ChildResult) -> bool {
    cr.epitaph == inbox::Epitaph::FinalResponse.as_str()
}

/// The workflow's cap on the landing's extract (ARCH §2.7, §6) — the one
/// place the landing's config key is named, so the interpreter above
/// hands over the `workflow.yaml` it already holds rather than a number
/// it had to know to read. Absent block, absent key: no extract.
fn extract_bytes(workflow: &Workflow) -> Option<usize> {
    workflow
        .compaction
        .as_ref()
        .and_then(|c| c.intermediate.extract_bytes)
}

/// `land_compaction` (§2.6): land the returning compactor's product by
/// rebase-forward — the compaction base plus the replayed live tail —
/// then consume the trigger message (the base commit is the record —
/// never a transcript entry).
///
/// A replay git could not resolve is **declined** by [`compactor::land`]
/// — aborted and marked at `refs/litany/conflicted/<compactor-id>` — and
/// reported here for the operator; a pass another landing overtook is
/// **superseded** and reported without a mark (not a defect — the next
/// checkpoint trigger fires afresh). Both reports are operator notices
/// (`crate::prompt::notice`) and carry its prefix: a driver's stderr is
/// captured, so the reader is a program (§2.11). The trigger message is
/// consumed in every case: the compactor has returned, and re-reading
/// its result would re-attempt a landing whose outcome is already
/// recorded.
pub(super) fn land(
    worktree: &Path,
    agent_id: &str,
    cr: &ChildResult,
    workflow: &Workflow,
    git: &dyn GitRunner,
) -> Result<(), Error> {
    match compactor::land(
        worktree,
        agent_id,
        &cr.child_id,
        extract_bytes(workflow),
        git,
    )? {
        compactor::LandOutcome::Conflicted(paths) => notice!(
            "compaction landing [{}] declined — git could not replay {} \
             (marked refs/litany/conflicted/{}, ARCH §2.6); the branch continues uncompacted",
            cr.child_id,
            paths.join(", "),
            cr.child_id,
        ),
        compactor::LandOutcome::Superseded => notice!(
            "compaction landing [{}] superseded — a compaction landed since \
             its fork point (ARCH §2.6); the branch continues",
            cr.child_id,
        ),
        compactor::LandOutcome::Landed | compactor::LandOutcome::NoOp => {}
    }
    std::fs::remove_file(&cr.path).map_err(Error::Io)
}

/// `land_response(<file>)` (`docs/DESIGN_PROCEDURES.md` §4): write the
/// returning child's terminal response to `file` at the dispatcher's
/// branch root, commit it — subject `landing: <file> [<child-id>]`, the
/// one record; a later landing overwrites and `git log` on the path is
/// the history — and consume the result message. No work-product
/// transfer and no transcript entry: the file is out of band for the
/// dispatcher's model too, unless its manifest names it (ARCH §5.2).
/// Only a qualifying return reaches here ([`qualifies`]); the caller
/// delivers the rest as the obituaries they are.
pub(super) fn land_response(
    worktree: &Path,
    cr: &ChildResult,
    file: &str,
    git: &dyn GitRunner,
) -> Result<(), Error> {
    let mut body = cr.response.clone().unwrap_or_default();
    if !body.ends_with('\n') {
        body.push('\n');
    }
    std::fs::write(worktree.join(file), body).map_err(Error::Io)?;
    git.run(worktree, &["add", file])
        .map_err(|source| Error::Git {
            op: "landing add",
            source,
        })?;
    let subject = format!("landing: {file} [{}]", cr.child_id);
    git.run(worktree, &["commit", "-m", subject.as_str()])
        .map_err(|source| Error::Git {
            op: "landing commit",
            source,
        })?;
    std::fs::remove_file(&cr.path).map_err(Error::Io)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;

    /// A git stub recording argv, failing the run whose first word is
    /// `fail_on`.
    struct RecGit {
        runs: RefCell<Vec<Vec<String>>>,
        fail_on: Option<&'static str>,
    }
    impl GitRunner for RecGit {
        fn run(&self, _d: &Path, args: &[&str]) -> std::io::Result<()> {
            self.runs
                .borrow_mut()
                .push(args.iter().map(|s| s.to_string()).collect());
            if self.fail_on == Some(args[0]) {
                return Err(std::io::Error::other("stub"));
            }
            Ok(())
        }
        fn run_capture(&self, _d: &Path, _a: &[&str]) -> std::io::Result<String> {
            unreachable!("a landing never captures")
        }
    }

    fn result(dir: &Path, response: Option<&str>) -> ChildResult {
        let path = dir.join("inbox-msg.md");
        std::fs::write(&path, "---\n---\n").unwrap();
        ChildResult {
            child_id: "p1-c1".into(),
            terminal_ref: "abc".into(),
            epitaph: inbox::Epitaph::FinalResponse.as_str().into(),
            response: response.map(str::to_string),
            path,
        }
    }

    #[test]
    fn the_response_lands_as_one_committed_file_and_the_message_is_consumed() {
        let dir = tempfile::tempdir().unwrap();
        let git = RecGit {
            runs: RefCell::new(Vec::new()),
            fail_on: None,
        };
        let cr = result(dir.path(), Some("Port the parser"));
        land_response(dir.path(), &cr, "title", &git).unwrap();
        assert_eq!(
            std::fs::read_to_string(dir.path().join("title")).unwrap(),
            "Port the parser\n",
            "a trailing newline is normalized on"
        );
        let runs = git.runs.borrow();
        assert_eq!(runs[0], vec!["add", "title"]);
        assert_eq!(runs[1], vec!["commit", "-m", "landing: title [p1-c1]"]);
        assert!(!cr.path.exists(), "consumed, never delivered");
    }

    #[test]
    fn a_silent_child_lands_an_empty_file() {
        // `final-response` with no body (the agent never spoke) still
        // qualifies; the file records that it said nothing.
        let dir = tempfile::tempdir().unwrap();
        let git = RecGit {
            runs: RefCell::new(Vec::new()),
            fail_on: None,
        };
        let cr = result(dir.path(), None);
        land_response(dir.path(), &cr, "title", &git).unwrap();
        assert_eq!(
            std::fs::read_to_string(dir.path().join("title")).unwrap(),
            "\n"
        );
    }

    #[test]
    fn git_failures_surface_as_the_named_op_and_the_message_stays() {
        let dir = tempfile::tempdir().unwrap();
        for (fail_on, op) in [("add", "landing add"), ("commit", "landing commit")] {
            let git = RecGit {
                runs: RefCell::new(Vec::new()),
                fail_on: Some(fail_on),
            };
            let cr = result(dir.path(), Some("x"));
            let err = land_response(dir.path(), &cr, "title", &git).unwrap_err();
            assert!(
                matches!(&err, Error::Git { op: o, .. } if *o == op),
                "{err:?}"
            );
            assert!(cr.path.exists(), "not consumed on a failed landing");
        }
    }

    #[test]
    fn an_unwritable_target_and_a_vanished_message_are_io_errors() {
        let dir = tempfile::tempdir().unwrap();
        let git = RecGit {
            runs: RefCell::new(Vec::new()),
            fail_on: None,
        };
        // The target is a directory: the write fails before any git op.
        std::fs::create_dir(dir.path().join("title")).unwrap();
        let cr = result(dir.path(), Some("x"));
        let err = land_response(dir.path(), &cr, "title", &git).unwrap_err();
        assert!(matches!(err, Error::Io(_)), "{err:?}");
        assert!(git.runs.borrow().is_empty());
        // The message vanished under the landing: the consume reports it.
        let mut gone = result(dir.path(), Some("x"));
        gone.path = dir.path().join("absent.md");
        let err = land_response(dir.path(), &gone, "other", &git).unwrap_err();
        assert!(matches!(err, Error::Io(_)), "{err:?}");
    }
}
