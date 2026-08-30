//! Result-message deposit tests (ARCH §2.6, §2.11): the `epitaph:` /
//! `terminal_ref:` frontmatter, the body-iff-spoke rule, and the
//! dispatcher-address derivation an obituary is sent to. *Which* inbox a
//! terminal event addresses is the executor's rule and is tested at its
//! own seam (`prompt::dispatch::result_deposit`).

use super::super::deposit::{Epitaph, deposit_result, returned_ref};
use super::super::{inbox_dir, parent_of};
use crate::prompt::Clock;
use crate::template::GitRunner;
use std::cell::RefCell;
use std::io;
use std::path::Path;
use tempfile::TempDir;

/// Recording [`GitRunner`]: every `run` invocation's args, so a test
/// asserts the returned-mark `update-ref` (its one git effect) without a
/// real repo. `fail` makes `run` error, for the Mark-arm test.
#[derive(Default)]
struct RecGit {
    runs: RefCell<Vec<Vec<String>>>,
    fail: bool,
}
impl GitRunner for RecGit {
    fn run(&self, _dest: &Path, args: &[&str]) -> io::Result<()> {
        self.runs
            .borrow_mut()
            .push(args.iter().map(|a| a.to_string()).collect());
        if self.fail {
            return Err(io::Error::other("update-ref refused"));
        }
        Ok(())
    }
    fn run_capture(&self, _dest: &Path, _args: &[&str]) -> io::Result<String> {
        unreachable!("result deposit never captures git output")
    }
}

struct FixedClock;
impl Clock for FixedClock {
    fn now_iso8601(&self) -> String {
        "2026-07-11T00:00:00Z".into()
    }
    fn now_compact(&self) -> String {
        unreachable!("result deposit never reads the compact clock")
    }
}

fn read(path: &Path) -> String {
    std::fs::read_to_string(path).unwrap()
}

#[test]
fn epitaph_spellings_are_hyphenated() {
    assert_eq!(Epitaph::FinalResponse.as_str(), "final-response");
    assert_eq!(Epitaph::Stopped.as_str(), "stopped");
    assert_eq!(Epitaph::BudgetExhausted.as_str(), "budget-exhausted");
    assert_eq!(Epitaph::Died.as_str(), "died");
}

#[test]
fn result_message_carries_epitaph_ref_and_body_when_spoke() {
    let ws = TempDir::new().unwrap();
    let path = deposit_result(
        ws.path(),
        "parent",
        "parent-child",
        Epitaph::FinalResponse,
        "abc123",
        Some("all done\n"),
        &FixedClock,
        &RecGit::default(),
    )
    .unwrap();

    // Deposited into the PARENT's inbox, sender-namespaced by the child.
    assert_eq!(
        path,
        inbox_dir(ws.path(), "parent").join("parent-child-001.md")
    );
    assert_eq!(
        read(&path),
        "---\nfrom: parent-child\ndeposited_at: 2026-07-11T00:00:00Z\n\
         epitaph: final-response\nterminal_ref: abc123\n---\nall done\n"
    );
}

#[test]
fn result_message_omits_body_when_agent_never_spoke() {
    let ws = TempDir::new().unwrap();
    let path = deposit_result(
        ws.path(),
        "parent",
        "parent-child",
        Epitaph::BudgetExhausted,
        "def456",
        None,
        &FixedClock,
        &RecGit::default(),
    )
    .unwrap();
    // The file ends at the closing frontmatter delimiter — no body.
    assert_eq!(
        read(&path),
        "---\nfrom: parent-child\ndeposited_at: 2026-07-11T00:00:00Z\n\
         epitaph: budget-exhausted\nterminal_ref: def456\n---\n"
    );
}

#[test]
fn parent_of_strips_the_last_descent_segment() {
    // Root: one `<ts>-<short>` segment (two tokens) — no parent.
    assert_eq!(parent_of("20260711T000000Z-a1b2c3d4"), None);
    // Child: parent + one segment.
    assert_eq!(
        parent_of("20260711T000000Z-a1b2c3d4-20260711T000001Z-e5f6a7b8").as_deref(),
        Some("20260711T000000Z-a1b2c3d4")
    );
    // Grandchild strips only the last segment.
    assert_eq!(parent_of("r-aa-c-bb-g-cc").as_deref(), Some("r-aa-c-bb"));
    // Degenerate short ids still obey the two-token rule.
    assert_eq!(parent_of("a-b"), None);
    assert_eq!(parent_of("solo"), None);
}

#[test]
fn a_result_deposit_writes_the_durable_returned_mark() {
    // The mark is the fact's one durable home (§8): message file and
    // delivered transcript entry are both consumable, so the deposit
    // itself records that the child returned — for every epitaph.
    let ws = TempDir::new().unwrap();
    let git = RecGit::default();
    deposit_result(
        ws.path(),
        "parent",
        "parent-child",
        Epitaph::Died,
        "tipsha",
        None,
        &FixedClock,
        &git,
    )
    .unwrap();
    assert_eq!(
        *git.runs.borrow(),
        vec![vec![
            "update-ref".to_string(),
            returned_ref("parent-child"),
            "tipsha".to_string(),
        ]]
    );
    assert_eq!(returned_ref("x-y"), "refs/litany/returned/x-y");
}

#[test]
fn a_failed_mark_write_surfaces_after_the_file_landed() {
    // Ordering: file first, mark second — in the crash window the file
    // itself is the sweep's evidence. A mark failure is loud (an
    // unmarked return is what the sweep would misread as a death), and
    // the deposited file remains.
    let ws = TempDir::new().unwrap();
    let git = RecGit {
        fail: true,
        ..RecGit::default()
    };
    let err = deposit_result(
        ws.path(),
        "parent",
        "parent-child",
        Epitaph::FinalResponse,
        "tipsha",
        Some("done"),
        &FixedClock,
        &git,
    )
    .unwrap_err();
    assert!(
        err.to_string()
            .contains("mark refs/litany/returned/parent-child"),
        "unexpected error: {err}"
    );
    assert!(
        inbox_dir(ws.path(), "parent")
            .join("parent-child-001.md")
            .exists(),
        "the deposit itself must survive a failed mark"
    );
}
