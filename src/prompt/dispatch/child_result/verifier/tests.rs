//! Unit tests for [`super::already_gated`]'s enumeration edges — the
//! branch-list failure tag and the blank-line skip. The behavioral
//! verdict paths run against real git in
//! [`super::super::tests::gate`]; these two edges need a git whose
//! `branch --list` output (or failure) is scripted.

use super::*;
use std::path::Path;

/// Scripted `branch --list`: `Some(out)` answers with `out`, `None`
/// fails the capture.
struct BranchList(Option<&'static str>);

impl GitRunner for BranchList {
    fn run(&self, _dest: &Path, _args: &[&str]) -> std::io::Result<()> {
        unreachable!("already_gated only captures")
    }
    fn run_capture(&self, _dest: &Path, args: &[&str]) -> std::io::Result<String> {
        assert_eq!(args.first(), Some(&"branch"), "unexpected git op: {args:?}");
        self.0
            .map(str::to_owned)
            .ok_or_else(|| std::io::Error::other("stub git failure"))
    }
}

#[test]
fn already_gated_tags_a_branch_list_failure_with_its_op() {
    let err = already_gated(
        Path::new("/nowhere"),
        "p1",
        "r1",
        VERIFIER_ROLE,
        &BranchList(None),
    )
    .unwrap_err();
    assert!(
        matches!(
            err,
            Error::Git {
                op: "gate branch list",
                ..
            }
        ),
        "{err:?}"
    );
}

#[test]
fn already_gated_skips_blank_branch_lines() {
    // A trailing/blank line in the listing is not a branch; nothing
    // else in the output, so no verifier gates the ref.
    let gated = already_gated(
        Path::new("/nowhere"),
        "p1",
        "r1",
        VERIFIER_ROLE,
        &BranchList(Some("\n  \n")),
    )
    .unwrap();
    assert!(!gated);
}
