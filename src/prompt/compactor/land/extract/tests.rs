//! The extract's own unit arms (`docs/DESIGN_CONTEXT_ECONOMY.md` §5.3):
//! one per section, the deduplication and newest-first order, the cap's
//! two behaviours, and the classifier's near-misses. The landing-level
//! arms — the base carrying the file, an omitted key writing none, the
//! pair shedding together — live in [`super::super::tests`].

use super::*;

/// Render the extract for entries given newest-first as
/// `(path, content)`, under `cap`.
fn extract(entries: &[(&str, &str)], cap: usize) -> String {
    let refs: Vec<scan::Refs> = entries.iter().map(|(p, c)| scan::of(p, c)).collect();
    render(&refs, cap)
}

/// A model-output entry carrying one text block.
fn said(text: &str) -> String {
    serde_json::json!([{"type": "text", "text": text}]).to_string()
}

/// A tool entry carrying one `tool_result` block.
fn tool_result(text: &str, is_error: bool) -> String {
    serde_json::json!([{
        "type": "tool_result",
        "tool_use_id": "t1",
        "is_error": is_error,
        "content": [{"type": "text", "text": text}],
    }])
    .to_string()
}

#[test]
fn a_user_message_rides_verbatim_under_its_own_section() {
    let out = extract(
        &[("messages/002-user.md", "ship the widget\nby friday\n")],
        4096,
    );
    assert!(out.contains("## verbatim user messages"), "{out}");
    assert!(out.contains("ship the widget\nby friday"), "{out}");
}

#[test]
fn a_message_from_a_child_is_not_a_user_message() {
    // §2.3 origins: the origin token is the sender, and only `user` names
    // the operator. A child's returned result is a message like any
    // other and is scanned for references, never quoted whole.
    let out = extract(&[("messages/002-p1-kid.md", "done: src/lib.rs\n")], 4096);
    assert!(!out.contains("## verbatim user messages"), "{out}");
    assert!(out.contains("- src/lib.rs"), "{out}");
}

#[test]
fn an_empty_user_message_contributes_nothing() {
    assert_eq!(extract(&[("messages/002-user.md", "  \n")], 4096), "");
}

#[test]
fn an_is_error_tool_results_last_line_is_the_error_string() {
    let out = extract(
        &[(
            "messages/003-tool.json",
            &tool_result("running tests\n\nerror: 2 tests failed\n", true),
        )],
        4096,
    );
    assert!(out.contains("## error strings"), "{out}");
    assert!(out.contains("- error: 2 tests failed\n"), "{out}");
    assert!(
        !out.contains("- running tests"),
        "only the last line: {out}"
    );
}

#[test]
fn a_successful_tool_result_contributes_no_error_string() {
    let out = extract(
        &[("messages/003-tool.json", &tool_result("ok #7\n", false))],
        4096,
    );
    assert!(!out.contains("## error strings"), "{out}");
    assert!(out.contains("- #7"), "still scanned for references: {out}");
}

#[test]
fn pull_requests_shas_and_paths_are_each_their_own_section() {
    let out = extract(
        &[(
            "messages/004-m.json",
            &said(
                "landed #12 as https://github.com/o/r/pull/34 \
                 in deadb1f, see (src/prompt/land.rs).",
            ),
        )],
        4096,
    );
    for want in [
        "## pull requests",
        "- #12\n",
        "- #34\n",
        "## commit shas",
        "- deadb1f\n",
        "## paths",
        "- src/prompt/land.rs\n",
    ] {
        assert!(out.contains(want), "{want} missing from {out}");
    }
}

#[test]
fn a_tool_uses_input_is_scanned_for_the_paths_it_names() {
    let blocks = serde_json::json!([{
        "type": "tool_use",
        "id": "t1",
        "name": "read_file",
        "input": {"path": "docs/ARCHITECTURE.md"},
    }])
    .to_string();
    let out = extract(&[("messages/005-m.json", &blocks)], 4096);
    assert!(out.contains("- docs/ARCHITECTURE.md\n"), "{out}");
}

#[test]
fn the_classifiers_near_misses_are_left_alone() {
    // A date stamp is not a sha (no hex letter); a hex-lettered word is
    // not a sha (no digit); a bare host is not a path (no extension on
    // its last segment); a URL is not a path (it has a scheme); a `#`
    // with no digits is not a pull request.
    let out = extract(
        &[(
            "messages/002-m.json",
            &said("20260904 deadbeef github.com/o/r https://x.test/a.html #hash"),
        )],
        4096,
    );
    assert_eq!(out, "", "nothing classified: {out}");
}

#[test]
fn references_are_deduplicated_and_newest_first() {
    // Entries arrive newest first, and the first sighting holds the
    // place — so the newest mention of a repeated path wins its slot and
    // the section reads newest to oldest.
    let out = extract(
        &[
            ("messages/004-m.json", &said("new.rs is at src/new.rs")),
            ("messages/003-m.json", &said("src/old.rs and src/new.rs")),
        ],
        4096,
    );
    let paths: Vec<&str> = out.lines().filter(|l| l.starts_with("- src/")).collect();
    assert_eq!(paths, vec!["- src/new.rs", "- src/old.rs"], "{out}");
}

#[test]
fn the_cap_cuts_the_first_overflowing_section_under_an_honest_marker() {
    let entry = said("src/a.rs src/b.rs src/c.rs");
    let out = extract(&[("messages/002-m.json", &entry)], 30);
    assert!(out.contains("- src/a.rs\n"), "{out}");
    assert!(!out.contains("- src/c.rs"), "{out}");
    assert!(
        out.contains("[... extract truncated at the 30-byte cap: 2 of 3 references omitted"),
        "{out}"
    );
    assert!(
        out.starts_with(PREAMBLE),
        "the preamble is structure: {out}"
    );
}

#[test]
fn an_extract_in_which_nothing_fits_is_not_written() {
    // Not a truncated extract but no extract: a file saying only that
    // everything was omitted tells the model nothing it can act on.
    assert_eq!(
        extract(&[("messages/002-m.json", &said("src/a.rs"))], 1),
        ""
    );
    assert_eq!(
        extract(&[("messages/002-m.json", &said("src/a.rs"))], 0),
        ""
    );
}

#[test]
fn nothing_removed_is_nothing_written() {
    assert_eq!(extract(&[], 4096), "");
}

#[test]
fn every_block_kind_contributes_what_it_carries_and_no_more() {
    // Thinking text is the model's own prose and names references like
    // any other; the opaque provider-executed and binary blocks carry
    // none and contribute nothing rather than their wire shape.
    let blocks = serde_json::json!([
        {"type": "thinking", "text": "check src/a.rs", "signature": "s"},
        {"type": "redacted_thinking", "data": "src/hidden.rs"},
        {"type": "image", "source": {"kind": "base64", "media_type": "image/png", "data": "q"}},
    ])
    .to_string();
    let out = extract(&[("messages/002-m.json", &blocks)], 4096);
    assert!(out.contains("- src/a.rs\n"), "{out}");
    assert!(!out.contains("hidden"), "opaque blocks stay opaque: {out}");
}

#[test]
fn an_error_result_with_no_text_states_no_error_string() {
    // The section is the last *line*; a result that wrote none has none
    // to state — the general path with empty inputs, not an empty bullet.
    let out = extract(
        &[("messages/003-tool.json", &tool_result("\n \n", true))],
        4096,
    );
    assert_eq!(out, "");
}

#[test]
fn a_dotfile_name_is_not_an_extension() {
    // `src/.rs` has no stem before the dot, so it names no file — the
    // same near-miss discipline as the bare host and the URL above.
    let out = extract(&[("messages/002-m.json", &said("src/.rs a/b.c-d"))], 4096);
    assert_eq!(out, "", "{out}");
}
