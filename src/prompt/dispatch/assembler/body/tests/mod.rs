//! Unit tests for §5.2 head-and-body composition: selection (globs,
//! structural skips, category order), the token budget, and each
//! overflow policy. Token math throughout uses 4-byte-multiple contents
//! so 1 token == 4 bytes exactly.

use super::*;
use crate::config::manifest::OverflowPolicy;
use tempfile::TempDir;

fn rules(pinned: &[&str], order: &[&str], budget: u32, overflow: OverflowPolicy) -> RoleRules {
    RoleRules {
        pinned: pinned.iter().map(|s| s.to_string()).collect(),
        order: order.iter().map(|s| s.to_string()).collect(),
        budget_tokens: budget,
        overflow,
    }
}

fn write(wt: &Path, rel: &str, bytes: &[u8]) {
    let path = wt.join(rel);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, bytes).unwrap();
}

/// The `path` attribute of each rendered block, for order assertions.
fn paths(blocks: &[String]) -> Vec<String> {
    blocks
        .iter()
        .map(|b| {
            let start = b.find('"').unwrap() + 1;
            b[start..b[start..].find('"').unwrap() + start].to_string()
        })
        .collect()
}

#[test]
fn no_manifest_rules_compose_nothing() {
    let wt = TempDir::new().unwrap();
    write(wt.path(), "summary/001.md", b"data");
    assert!(compose(wt.path(), None).unwrap().is_empty());
}

#[test]
fn an_absent_worktree_composes_nothing() {
    let r = rules(&[], &["**"], 100, OverflowPolicy::Drop);
    let out = compose(Path::new("/no/such/worktree"), Some(&r)).unwrap();
    assert!(out.is_empty());
}

#[test]
fn a_worktree_that_is_a_file_surfaces_io_error() {
    let dir = TempDir::new().unwrap();
    let file = dir.path().join("not-a-dir");
    std::fs::write(&file, b"x").unwrap();
    let r = rules(&[], &["**"], 100, OverflowPolicy::Drop);
    assert!(matches!(
        compose(&file, Some(&r)).unwrap_err(),
        Error::Io(_)
    ));
}

#[test]
fn an_unreadable_selected_file_surfaces_io_error() {
    // A dangling symlink walks as a file and fails the read (§5.1 — a
    // worktree entry that cannot compose is declined, not skipped).
    let wt = TempDir::new().unwrap();
    std::os::unix::fs::symlink("/no/such/target", wt.path().join("dangling.md")).unwrap();
    let r = rules(&["dangling.md"], &[], 100, OverflowPolicy::Drop);
    assert!(matches!(
        compose(wt.path(), Some(&r)).unwrap_err(),
        Error::Io(_)
    ));
}

#[test]
fn structurally_homed_trees_never_compose_as_body_text() {
    // goal.md/name/soul.md (system slot, §2.3 — the name as the identity
    // line §2.8 derives), descriptions/tools/** and the skill
    // descriptions those tools claim (tools array, §3.3), messages/**
    // (transcript tail, §5.2), .git — all invisible even to a catch-all
    // glob, so nothing with a structural home is ever sent twice.
    let wt = TempDir::new().unwrap();
    write(wt.path(), "goal.md", b"goal");
    write(wt.path(), "name", b"pale-otter\n");
    write(wt.path(), "soul.md", b"soul");
    write(wt.path(), "messages/001-user.md", b"hi");
    write(wt.path(), "descriptions/tools/bash.json", b"{}");
    write(wt.path(), "descriptions/skills/bash.md", b"name: bash");
    write(wt.path(), ".git/config", b"[core]");
    write(wt.path(), "notes.md", b"kept");
    let r = rules(&["**"], &[], 100, OverflowPolicy::Drop);
    let out = compose(wt.path(), Some(&r)).unwrap();
    assert_eq!(paths(&out), vec!["notes.md"]);
    assert_eq!(out[0], "<file path=\"notes.md\">\nkept\n</file>");
}

#[test]
fn a_standalone_skills_description_composes_as_a_head_block() {
    // §3.3 Description-always: a skill no tool claims has no tools-array
    // home, so `descriptions/**` carries it as ordinary head text — the
    // agent can discover it and elect `load_skill`.
    let wt = TempDir::new().unwrap();
    write(wt.path(), "descriptions/tools/bash.json", b"{}");
    write(wt.path(), "descriptions/skills/bash.md", b"tool");
    write(wt.path(), "descriptions/skills/git.md", b"alone");
    write(wt.path(), "descriptions/skills/README", b"n/a");
    let r = rules(&["descriptions/**"], &[], 100, OverflowPolicy::Drop);
    let out = compose(wt.path(), Some(&r)).unwrap();
    let kept = ["descriptions/skills/README", "descriptions/skills/git.md"];
    assert_eq!(paths(&out), kept);
}

#[test]
fn body_fills_in_category_order_lexical_within_each() {
    let wt = TempDir::new().unwrap();
    write(wt.path(), "skills/z/SKILL.md", b"zzzz");
    write(wt.path(), "skills/a/SKILL.md", b"aaaa");
    write(wt.path(), "summary/002.md", b"s2s2");
    write(wt.path(), "summary/001.md", b"s1s1");
    let r = rules(&[], &["summary/**", "skills/**"], 100, OverflowPolicy::Drop);
    let out = compose(wt.path(), Some(&r)).unwrap();
    assert_eq!(
        paths(&out),
        vec![
            "summary/001.md",
            "summary/002.md",
            "skills/a/SKILL.md",
            "skills/z/SKILL.md"
        ]
    );
}

#[test]
fn a_pinned_file_never_reenters_through_order() {
    let wt = TempDir::new().unwrap();
    write(wt.path(), "notes.md", b"once");
    let r = rules(&["notes.md"], &["**"], 100, OverflowPolicy::Drop);
    let out = compose(wt.path(), Some(&r)).unwrap();
    assert_eq!(paths(&out), vec!["notes.md"]);
}

#[test]
fn pinned_rides_over_budget_and_counts_toward_it() {
    // §5.2: pinned is always included regardless of budget, and what it
    // spends is gone — the body's allowance is the remainder.
    let wt = TempDir::new().unwrap();
    write(wt.path(), "notes.md", b"12345678"); // 2 tokens > budget 1
    write(wt.path(), "docs/a.md", b"data");
    let r = rules(&["notes.md"], &["docs/**"], 1, OverflowPolicy::Drop);
    let out = compose(wt.path(), Some(&r)).unwrap();
    assert_eq!(paths(&out), vec!["notes.md"]);
}

#[test]
fn a_fitting_body_passes_untouched() {
    let wt = TempDir::new().unwrap();
    write(wt.path(), "docs/a.md", b"data");
    write(wt.path(), "docs/b.md", b"data");
    let r = rules(&[], &["docs/**"], 2, OverflowPolicy::Drop);
    assert_eq!(
        paths(&compose(wt.path(), Some(&r)).unwrap()),
        vec!["docs/a.md", "docs/b.md"]
    );
}

#[test]
fn drop_oldest_summaries_sheds_lexically_first_until_the_body_fits() {
    let wt = TempDir::new().unwrap();
    write(wt.path(), "summary/001.md", b"12345678"); // 2 tokens
    write(wt.path(), "summary/002.md", b"12345678"); // 2 tokens
    write(wt.path(), "skills/a.md", b"data"); // 1 token
    let r = rules(
        &[],
        &["summary/**", "skills/**"],
        3,
        OverflowPolicy::DropOldestSummaries,
    );
    let out = compose(wt.path(), Some(&r)).unwrap();
    assert_eq!(paths(&out), vec!["summary/002.md", "skills/a.md"]);
}

#[test]
fn drop_oldest_summaries_sheds_a_summary_and_its_extract_together() {
    // `docs/DESIGN_CONTEXT_ECONOMY.md` §5.3: the landing's extract is
    // named so it sorts immediately after the summary it belongs to
    // (`001.md` < `001.refs.md` < `002.md`), so the age order this policy
    // sheds by carries the pair as one — never a `.refs.md` orphaned
    // from the prose it annotates.
    let wt = TempDir::new().unwrap();
    write(wt.path(), "summary/001.md", b"12345678"); // 2 tokens
    write(wt.path(), "summary/001.refs.md", b"1234"); // 1 token
    write(wt.path(), "summary/002.md", b"12345678"); // 2 tokens
    let r = rules(&[], &["summary/**"], 2, OverflowPolicy::DropOldestSummaries);
    let out = compose(wt.path(), Some(&r)).unwrap();
    assert_eq!(paths(&out), vec!["summary/002.md"]);
}

#[test]
fn drop_oldest_summaries_with_none_left_lets_the_residue_ride() {
    let wt = TempDir::new().unwrap();
    write(wt.path(), "skills/a.md", b"12345678"); // 2 tokens > budget 1
    let r = rules(&[], &["skills/**"], 1, OverflowPolicy::DropOldestSummaries);
    assert_eq!(
        paths(&compose(wt.path(), Some(&r)).unwrap()),
        vec!["skills/a.md"]
    );
}

#[test]
fn truncate_cuts_the_overflowing_entry_at_a_char_boundary_and_stops() {
    let wt = TempDir::new().unwrap();
    write(wt.path(), "docs/a.md", b"12345678"); // 2 tokens
    write(wt.path(), "docs/b.md", "abc\u{e9}xxxx".as_bytes()); // 9 bytes, 3 tokens
    write(wt.path(), "docs/c.md", b"data"); // would fit, never reached
    let r = rules(&[], &["docs/**"], 3, OverflowPolicy::Truncate);
    let out = compose(wt.path(), Some(&r)).unwrap();
    // b's 1-token allowance is 4 bytes — inside the 2-byte é, backed
    // off to the boundary at 3.
    assert_eq!(
        out,
        vec![
            "<file path=\"docs/a.md\">\n12345678\n</file>".to_string(),
            "<file path=\"docs/b.md\">\nabc\n</file>".to_string(),
        ]
    );
}

#[test]
fn truncate_with_no_allowance_left_drops_the_overflowing_entry() {
    let wt = TempDir::new().unwrap();
    write(wt.path(), "docs/a.md", b"12345678"); // spends the whole budget
    write(wt.path(), "docs/b.md", b"data");
    let r = rules(&[], &["docs/**"], 2, OverflowPolicy::Truncate);
    assert_eq!(
        paths(&compose(wt.path(), Some(&r)).unwrap()),
        vec!["docs/a.md"]
    );
}

#[test]
fn drop_stops_filling_at_the_first_entry_that_does_not_fit() {
    let wt = TempDir::new().unwrap();
    write(wt.path(), "docs/a.md", b"12345678"); // 2 tokens
    write(wt.path(), "docs/b.md", b"12345678"); // overflows at 3
    write(wt.path(), "docs/c.md", b"data"); // would fit, never reached
    let r = rules(&[], &["docs/**"], 3, OverflowPolicy::Drop);
    assert_eq!(
        paths(&compose(wt.path(), Some(&r)).unwrap()),
        vec!["docs/a.md"]
    );
}

#[test]
fn non_utf8_content_composes_lossily() {
    let wt = TempDir::new().unwrap();
    write(wt.path(), "blob.bin", &[0xff, b'o', b'k']);
    let r = rules(&["blob.bin"], &[], 100, OverflowPolicy::Drop);
    let out = compose(wt.path(), Some(&r)).unwrap();
    assert!(out[0].contains("\u{fffd}ok"));
}

mod markers;

#[test]
fn the_shipped_worker_rules_compose_the_facts_file_as_a_head_block() {
    // The pin's end-to-end claim (ARCH §5.5): the shipped `worker`
    // manifest entry selects `facts.md`, and selection here means one
    // path-framed head block ahead of every ordered category — the
    // durable memory at the head of the call, never shed by the body's
    // budget (§5.2).
    let raw = crate::template::TEMPLATE
        .get_file("manifest.yaml")
        .expect("the template ships manifest.yaml")
        .contents_utf8()
        .expect("manifest.yaml is UTF-8");
    let shipped =
        crate::config::manifest::Manifest::parse(raw, Path::new("template/manifest.yaml"))
            .expect("the shipped template parses");
    let wt = TempDir::new().unwrap();
    write(wt.path(), crate::facts::FILE, b"the build runs on nightly");
    write(wt.path(), "summary/001.md", b"an ordered body entry");

    let blocks = compose(wt.path(), Some(&shipped.roles["worker"])).unwrap();

    assert_eq!(
        blocks.first().map(String::as_str),
        Some("<file path=\"facts.md\">\nthe build runs on nightly\n</file>")
    );
    assert_eq!(paths(&blocks), vec!["facts.md", "summary/001.md"]);
}
