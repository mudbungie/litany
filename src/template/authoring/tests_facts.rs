//! The facts file's write-side cap (ARCH §5.5,
//! `docs/DESIGN_CONTEXT_ECONOMY.md` §3): a config commit whose
//! `facts.md` is over [`crate::facts::MAX_BYTES`] is refused, because a
//! pinned path is never shed at assembly and nothing downstream can
//! make an oversized one fit.

use super::tests::{show, workspace, write_files};
use super::{Error, Origin, author};
use crate::facts::{FILE, MAX_BYTES, OverCap};
use crate::template::RealGit;

/// `n` bytes of facts, as an edit closure's file set.
fn facts_of(n: u64) -> String {
    "x".repeat(usize::try_from(n).unwrap())
}

#[test]
fn an_over_cap_facts_file_is_refused_and_the_branch_does_not_move() {
    let (holder, ws) = workspace();
    let head = show(&ws, "config/default:version").unwrap();
    let body = facts_of(MAX_BYTES + 1);

    let err = author(
        &ws,
        &holder.path().join("no-pool"),
        "default",
        Origin::Advance,
        write_files(&[(FILE, body.as_str())]),
        &RealGit::new(),
    )
    .unwrap_err();

    assert!(
        matches!(&err, Error::Facts(OverCap::TooLarge { bytes }) if *bytes == MAX_BYTES + 1),
        "{err:?}"
    );
    // The refusal names both numbers, and the pass left nothing behind:
    // no commit, no moved branch, no checkout to wedge the next one.
    let text = err.to_string();
    assert!(text.contains(&(MAX_BYTES + 1).to_string()), "{text}");
    assert!(text.contains(&MAX_BYTES.to_string()), "{text}");
    assert_eq!(show(&ws, "config/default:version").unwrap(), head);
    assert!(show(&ws, &format!("config/default:{FILE}")).is_err());
    assert!(!ws.join(".config-author").exists(), "checkout must be gone");
}

#[test]
fn a_facts_file_at_the_cap_lands_on_the_lineage() {
    let (holder, ws) = workspace();
    let body = facts_of(MAX_BYTES);

    author(
        &ws,
        &holder.path().join("no-pool"),
        "default",
        Origin::Advance,
        write_files(&[(FILE, body.as_str())]),
        &RealGit::new(),
    )
    .unwrap();

    assert_eq!(
        show(&ws, &format!("config/default:{FILE}")).unwrap().len(),
        usize::try_from(MAX_BYTES).unwrap()
    );
}
