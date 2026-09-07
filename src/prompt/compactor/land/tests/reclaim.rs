//! **A landed compaction reclaims the span** (bl-2071,
//! `docs/DESIGN_CONTEXT_ECONOMY.md` §5.5) — the arm the shipped
//! mechanism did not have.
//!
//! Four landed compactions over 94 transcript commits freed zero bytes,
//! because the only thing that removed a transcript entry was a model
//! nominating it and the soul never said to. These arms hold the landing
//! to the property directly: the base's `messages/` holds the dispatch
//! entry and nothing else, and the assembled prompt — the actual wire
//! history, measured through [`crate::prompt::dispatch::assembler`] —
//! is smaller after the landing than before it.

use super::*;
use crate::prompt::dispatch::assembler;
use brazen::Content;

/// Serialized bytes of the wire message history assembled from the
/// branch's worktree — transcript-only (`rules: None`), which is what a
/// compaction can move. The unit is the prompt's own, not a file count:
/// a landing that "reclaimed" by deleting one small entry and adding a
/// large summary would pass a count and fail this.
fn prompt_bytes(wt: &Path) -> usize {
    let messages = assembler::assemble(wt, None).unwrap().messages;
    serde_json::to_vec(&messages).unwrap().len()
}

/// A branch with a dispatch entry and `n` further transcript entries,
/// each carrying `filler` so the span is worth reclaiming.
fn talkative(n: u32) -> TempDir {
    let dir = repo(&[("messages/001-user.md", "the opening prompt\n")]);
    let wt = dir.path();
    for seq in 2..=n {
        let body = format!("entry {seq}: {}\n", "chatter ".repeat(40));
        commit(
            wt,
            &format!("step {seq:03}"),
            &[(&format!("messages/{seq:03}-user.md"), body.as_str())],
            &[],
        );
    }
    dir
}

/// Every `messages/**` path in the branch's worktree, sorted.
fn entries(wt: &Path) -> Vec<String> {
    let mut found: Vec<String> = std::fs::read_dir(wt.join("messages"))
        .map(|rd| {
            rd.filter_map(|e| e.ok())
                .map(|e| e.file_name().to_string_lossy().into_owned())
                .collect()
        })
        .unwrap_or_default();
    found.sort();
    found
}

#[test]
fn a_landed_compaction_shrinks_the_assembled_prompt() {
    // THE PIN (bl-2071): the compactor nominates NOTHING — exactly what
    // every compactor in the measured run did — and the landing still
    // reclaims the span, because the summary is what stands in for it.
    let dir = talkative(9);
    let wt = dir.path();
    let before = prompt_bytes(wt);
    compactor(
        wt,
        &[("summary/001.md", "we talked about widgets\n")],
        &[],
        &[],
        &[],
    );

    assert_eq!(
        land(wt, "p1", "p1-cmp", None, &g()).unwrap(),
        LandOutcome::Landed
    );

    let after = prompt_bytes(wt);
    assert!(
        after < before,
        "a landed compaction must shrink the prompt: {before} -> {after}"
    );
    // The invariant is readable off the base alone: the dispatch entry
    // stands (§2.7 — the operator's only copy of the opening prompt),
    // and every entry the summary replaced is gone.
    assert_eq!(entries(wt), vec!["001-user.md".to_string()]);
    assert!(wt.join("summary/001.md").exists(), "the summary landed");
    assert_eq!(g().run_capture(wt, &["status", "--porcelain"]).unwrap(), "");
}

#[test]
fn the_live_tail_past_the_compaction_point_is_never_swept() {
    // The retained tail is structural, not a second rule: `keep_recent`
    // moves the compaction point back, and the sweep reads the point's
    // tree. Entries appended while the compactor ran replay untouched.
    let dir = talkative(4);
    let wt = dir.path();
    compactor(wt, &[("summary/001.md", "digest\n")], &[], &[], &[]);
    commit(wt, "step 005", &[("messages/005-user.md", "live\n")], &[]);

    assert_eq!(
        land(wt, "p1", "p1-cmp", None, &g()).unwrap(),
        LandOutcome::Landed
    );
    assert_eq!(
        entries(wt),
        vec!["001-user.md".to_string(), "005-user.md".to_string()]
    );
}

#[test]
fn a_pass_that_wrote_no_summary_sweeps_nothing() {
    // The sweep is conditional on the summary and on nothing else: with
    // no summary there is nothing standing in for the span, so a
    // deletions-only pass carries away only what it named.
    let dir = talkative(4);
    let wt = dir.path();
    compactor(wt, &[], &["messages/003-user.md"], &[], &[]);

    assert_eq!(
        land(wt, "p1", "p1-cmp", None, &g()).unwrap(),
        LandOutcome::Landed
    );
    assert_eq!(
        entries(wt),
        vec![
            "001-user.md".to_string(),
            "002-user.md".to_string(),
            "004-user.md".to_string()
        ]
    );
}

#[test]
fn the_extract_reports_the_swept_span_not_only_what_was_nominated() {
    // The extract derives from what the compaction removes from context
    // (§5.3), and since bl-2071 that is the span — so a pass that
    // nominated nothing still lands references for every entry it took.
    let dir = repo(&[
        ("messages/001-user.md", "the opening prompt\n"),
        ("messages/002-user.md", "ship the widget\n"),
    ]);
    let wt = dir.path();
    compactor(wt, &[("summary/001.md", "digest\n")], &[], &[], &[]);

    assert_eq!(
        land(wt, "p1", "p1-cmp", Some(4096), &g()).unwrap(),
        LandOutcome::Landed
    );
    let refs = std::fs::read_to_string(wt.join("summary/001.refs.md")).unwrap();
    assert!(refs.contains("ship the widget"), "{refs}");
    assert!(
        !refs.contains("the opening prompt"),
        "the dispatch entry never leaves context: {refs}"
    );
}

/// A model-output entry carrying one `tool_use`, as the executor commits
/// it before any tool runs (§2.5).
fn call_entry(id: &str) -> String {
    serde_json::to_string(&[Content::ToolUse {
        id: id.to_string(),
        name: "bash".into(),
        input: serde_json::json!({"command": "true"}),
        signature: None,
    }])
    .unwrap()
}

/// The `messages/NNN-tool.json` answering it, committed after the tool
/// returned — which is after the compaction point was taken.
fn result_entry(id: &str) -> String {
    serde_json::to_string(&[Content::ToolResult {
        tool_use_id: id.to_string(),
        content: vec![Content::Text("ok".into())],
        is_error: false,
    }])
    .unwrap()
}

/// Every `tool_use_id` the branch's **record** carries with no
/// `tool_use` before it — what the provider refuses ("No tool call found
/// for function call output with call_id …").
///
/// Read off the transcript files rather than through
/// [`assembler::assemble`] on purpose: assembly's own orphan drop is the
/// backstop for a branch already wedged (bl-2d93), and measuring through
/// it would make this beat pass no matter what the landing did.
fn orphans(wt: &Path) -> Vec<String> {
    let mut called = std::collections::HashSet::new();
    let mut orphaned = Vec::new();
    for name in entries(wt).iter().filter(|n| n.ends_with(".json")) {
        let Ok(bytes) = std::fs::read(wt.join("messages").join(name)) else {
            continue;
        };
        for b in crate::prompt::dispatch::entry::blocks(&bytes) {
            match b {
                Content::ToolUse { id, .. } => {
                    called.insert(id);
                }
                Content::ToolResult { tool_use_id, .. } if !called.contains(&tool_use_id) => {
                    orphaned.push(tool_use_id);
                }
                _ => {}
            }
        }
    }
    orphaned
}

#[test]
fn a_compaction_point_inside_a_tool_window_never_orphans_the_result() {
    // THE REPRO (bl-2d93): the point is an arbitrary commit —
    // `HEAD~keep_recent` counts commits, the token tail lands on a model
    // entry's own commit — and a tool window spans several, so the point
    // falls between the call and its result. Swept flat, the call leaves
    // the base and the result replays on top of it, and every later
    // prompt on that branch dies at the provider.
    let dir = repo(&[("messages/001-user.md", "the opening prompt\n")]);
    let wt = dir.path();
    commit(
        wt,
        "step 002",
        &[("messages/002-m.json", call_entry("call_1").as_str())],
        &[],
    );
    // The compactor forks HERE — mid-window — and the tool returns while
    // it runs.
    compactor(wt, &[("summary/001.md", "digest\n")], &[], &[], &[]);
    commit(
        wt,
        "step 003",
        &[("messages/003-tool.json", result_entry("call_1").as_str())],
        &[],
    );

    assert_eq!(
        land(wt, "p1", "p1-cmp", None, &g()).unwrap(),
        LandOutcome::Landed
    );
    assert_eq!(orphans(wt), Vec::<String>::new());
    // The unit stays whole: the call is still in context beside its
    // result, and the settled entries before it are still reclaimed.
    assert_eq!(
        entries(wt),
        vec![
            "001-user.md".to_string(),
            "002-m.json".to_string(),
            "003-tool.json".to_string()
        ]
    );
}

#[test]
fn a_settled_window_inside_the_span_is_swept_whole() {
    // The other direction: a window that closed before the point is
    // ordinary span content, and the sweep takes both halves of it.
    let dir = repo(&[("messages/001-user.md", "the opening prompt\n")]);
    let wt = dir.path();
    commit(
        wt,
        "step 002",
        &[("messages/002-m.json", call_entry("call_1").as_str())],
        &[],
    );
    commit(
        wt,
        "step 003",
        &[("messages/003-tool.json", result_entry("call_1").as_str())],
        &[],
    );
    compactor(wt, &[("summary/001.md", "digest\n")], &[], &[], &[]);

    assert_eq!(
        land(wt, "p1", "p1-cmp", None, &g()).unwrap(),
        LandOutcome::Landed
    );
    assert_eq!(entries(wt), vec!["001-user.md".to_string()]);
}
