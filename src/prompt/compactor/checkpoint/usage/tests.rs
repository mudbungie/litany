//! The last-usage read and the `window_percent` predicate over it
//! (`docs/DESIGN_CONTEXT_ECONOMY.md` §5.1).

use super::*;
use tempfile::TempDir;

fn write(dir: &Path, name: &str, body: &str) {
    let messages = dir.join(MESSAGES_DIR);
    std::fs::create_dir_all(&messages).expect("messages/");
    std::fs::write(messages.join(name), body).expect("entry");
}

fn usage_of(prompt: u64, window: Option<u64>, model: &str) -> LastUsage {
    LastUsage {
        prompt_tokens: prompt,
        context_window: window,
        model: model.to_string(),
    }
}

#[test]
fn the_prompt_side_sums_every_reported_counter_and_ignores_the_output() {
    // §5.1: input + cache_read + cache_write, and *not* output_tokens —
    // the trigger measures what the next call must re-send, which is the
    // prompt side alone.
    let dir = TempDir::new().unwrap();
    write(
        dir.path(),
        "004-claude-fable-5.json",
        r#"{"content":[],"usage":{"input_tokens":10,"cache_read_tokens":20,
            "cache_write_tokens":5,"output_tokens":900,"context_window":1000}}"#,
    );
    let last = last(dir.path()).unwrap().expect("a model entry");
    assert_eq!(last.prompt_tokens, 35);
    assert_eq!(last.context_window, Some(1000));
    assert_eq!(last.model, "claude-fable-5");
}

#[test]
fn an_absent_counter_contributes_nothing_rather_than_zero() {
    // brazen's zero-vs-unknown rule (§2.3): a counter the provider never
    // stated is missing from the report, and the sum simply skips it.
    let dir = TempDir::new().unwrap();
    write(
        dir.path(),
        "001-m.json",
        r#"{"content":[],"usage":{"input_tokens":7}}"#,
    );
    let last = last(dir.path()).unwrap().expect("a model entry");
    assert_eq!(last.prompt_tokens, 7);
    assert_eq!(last.context_window, None);
}

#[test]
fn the_newest_model_entry_answers_past_tool_and_delivery_entries() {
    // The transcript's order lives in the `NNN` prefix (§2.3), and only a
    // model entry carries usage: the reserved `tool` origin is a
    // tool_result, and a `.md` is a delivered message.
    let dir = TempDir::new().unwrap();
    write(
        dir.path(),
        "001-m.json",
        r#"{"content":[],"usage":{"input_tokens":1}}"#,
    );
    write(
        dir.path(),
        "010-m.json",
        r#"{"content":[],"usage":{"input_tokens":99}}"#,
    );
    write(
        dir.path(),
        "011-tool.json",
        r#"[{"type":"text","text":"t"}]"#,
    );
    write(dir.path(), "012-someone.md", "hello");
    write(dir.path(), "not-an-entry.json", "{}");
    write(dir.path(), "013-.json", "{}");
    let last = last(dir.path()).unwrap().expect("a model entry");
    assert_eq!(last.prompt_tokens, 99);
}

#[test]
fn a_branch_with_no_transcript_or_no_usage_reports_nothing() {
    // Three empty-input paths, all "no last usage": no `messages/` at all
    // (step 1, before the fork's first commit), a directory holding no
    // model entry, and the lawful bare-array entry shape (§2.3).
    let dir = TempDir::new().unwrap();
    assert_eq!(last(dir.path()).unwrap(), None);
    write(dir.path(), "001-someone.md", "hi");
    assert_eq!(last(dir.path()).unwrap(), None);
    write(dir.path(), "002-m.json", r#"[{"type":"text","text":"hi"}]"#);
    assert_eq!(last(dir.path()).unwrap(), None);
}

#[test]
fn an_unreadable_transcript_directory_surfaces_as_io() {
    // `messages` occupied by a file is not "no transcript": the read
    // fails for a reason the caller must see, so only NotFound is the
    // empty path.
    let dir = TempDir::new().unwrap();
    std::fs::write(dir.path().join(MESSAGES_DIR), "not a directory").unwrap();
    assert!(matches!(last(dir.path()), Err(Error::Io(_))));
}

#[test]
fn the_window_trigger_fires_at_or_past_the_percentage() {
    // 50% of a 1000-token window is 500: 499 is not due, 500 is, and the
    // comparison is a product, so no division rounds the boundary away.
    let due_at = |prompt| due(Some(50), Some(&usage_of(prompt, Some(1000), "m"))).unwrap();
    assert!(!due_at(499));
    assert!(due_at(500));
    assert!(due_at(999));
}

#[test]
fn no_last_usage_and_no_threshold_are_both_not_due() {
    // Step 1 has no model entry yet; a malformed `n` (guarded at config
    // load, §6) fails closed the same way every other trigger does.
    assert!(!due(Some(50), None).unwrap());
    assert!(!due(None, Some(&usage_of(999, Some(1000), "m"))).unwrap());
    assert!(!due(Some(0), Some(&usage_of(999, Some(1000), "m"))).unwrap());
}

#[test]
fn an_unknown_window_is_declined_naming_the_model() {
    // §5.1: never a trigger that silently never fires — the boundary
    // refuses, and the message carries the model whose window is absent
    // so the operator knows which row to change.
    let err = due(Some(50), Some(&usage_of(999, None, "gpt-mystery"))).unwrap_err();
    assert!(
        matches!(&err, Error::CompactionWindowUnknown { model } if model == "gpt-mystery"),
        "{err:?}"
    );
    assert!(format!("{err}").contains("window_percent"), "{err}");
}

#[test]
fn both_numbers_are_read_at_brazens_own_serialized_names() {
    // The entry's `usage` object is `brazen::Usage` serialized verbatim
    // by the transcript writer ([`crate::prompt::dispatch::entry`]), so
    // the names this module greps are not a transcription: this builds
    // the report from the adapter's own type and seals it the way the
    // writer does, which is what makes an additive `v=1` counter — the
    // window is one — ride through with no edit on either side.
    let mut reported = brazen::Usage::default();
    reported.input_tokens = Some(11);
    reported.cache_read_tokens = Some(22);
    reported.context_window = Some(200_000);
    let sealed = format!(
        r#"{{"content":[],"usage":{}}}"#,
        serde_json::to_string(&reported).expect("Usage serializes")
    );
    let dir = TempDir::new().unwrap();
    write(dir.path(), "007-claude-fable-5.json", &sealed);
    let report = last(dir.path()).unwrap().expect("a model entry");
    assert_eq!(report.prompt_tokens, 33);
    assert_eq!(report.context_window, Some(200_000));
    // And a row brazen states no window for stays absent, never a `0` or
    // a `null`: the decline path, not a silently unreachable threshold.
    let mut window_less = brazen::Usage::default();
    window_less.input_tokens = Some(11);
    let sealed = format!(
        r#"{{"content":[],"usage":{}}}"#,
        serde_json::to_string(&window_less).expect("Usage serializes")
    );
    write(dir.path(), "008-claude-fable-5.json", &sealed);
    let report = last(dir.path()).unwrap().expect("a model entry");
    assert_eq!(report.context_window, None);
    assert!(due(Some(50), Some(&report)).is_err());
}
