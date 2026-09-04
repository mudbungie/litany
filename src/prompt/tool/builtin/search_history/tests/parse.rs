//! The listing parser's contract (`super::super::parse`): what a raw
//! `git log --raw` line is, and the one-entry-one-hit rule the
//! rebase-forward landing makes necessary (§2.6, §4).

use super::super::parse;

/// A raw line for `path` adding `blob`, in git's own shape.
fn raw(blob: &str, path: &str) -> String {
    format!(":000000 100644 {} {blob} A\t{path}", "0".repeat(40))
}

#[test]
fn a_commit_line_owns_the_raw_lines_under_it() {
    let log = format!(
        "aaa\n\n{}\n{}\n\nbbb\n\n{}\n",
        raw("b1", "messages/001-user.md"),
        raw("b2", "summary/001.md"),
        raw("b3", "messages/002-user.md"),
    );
    let hits = parse(&log);
    let addrs: Vec<String> = hits.iter().map(super::super::Hit::address).collect();
    assert_eq!(
        addrs,
        [
            "aaa:messages/001-user.md",
            "aaa:summary/001.md",
            "bbb:messages/002-user.md"
        ]
    );
}

#[test]
fn one_entry_is_one_hit_however_many_commits_added_it() {
    // The replayed tail of a compacted branch: the same path and the
    // same bytes, added again by the landing's replay. The newest
    // address wins — the live branch's copy, not the archive's.
    let log = format!(
        "new\n\n{}\n\nold\n\n{}\n",
        raw("b1", "messages/001-user.md"),
        raw("b1", "messages/001-user.md"),
    );
    let hits = parse(&log);
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].address(), "new:messages/001-user.md");
}

#[test]
fn a_same_path_different_bytes_entry_is_its_own_hit() {
    // Two root agents each with their own `messages/001-user.md`: same
    // path, different bytes, two entries.
    let log = format!(
        "a\n\n{}\n\nb\n\n{}\n",
        raw("b1", "messages/001-user.md"),
        raw("b2", "messages/001-user.md"),
    );
    assert_eq!(parse(&log).len(), 2);
}

#[test]
fn a_raw_line_that_is_not_one_contributes_no_hit() {
    // Neither shape can come out of `git log --raw`, and the parse says
    // so by skipping rather than by guessing a path or a blob.
    let log = ":000000 100644 no tab here\n:too few\tmessages/001-user.md\n";
    assert!(parse(log).is_empty());
}
