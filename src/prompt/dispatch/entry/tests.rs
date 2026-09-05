//! Unit tests for the committed transcript entry's shape (ARCH §2.3):
//! both lawful read shapes, and the usage report's per-counter fold.

use super::*;
use serde_json::json;

/// The whole sealed entry object for `blocks_json` and a folded report.
fn sealed(blocks_json: &str, usage: &UsageReport) -> Value {
    let mut bytes = open().to_vec();
    bytes.extend_from_slice(blocks_json.as_bytes());
    bytes.extend_from_slice(&close(usage));
    serde_json::from_slice(&bytes).expect("a sealed entry is valid JSON")
}

fn usage_of(input: Option<u32>, output: Option<u32>) -> Usage {
    let mut u = Usage::default();
    u.input_tokens = input;
    u.output_tokens = output;
    u
}

#[test]
fn a_bare_block_array_parses() {
    assert_eq!(
        blocks(br#"[{"type":"text","text":"legacy"}]"#),
        vec![Content::Text("legacy".into())]
    );
}

#[test]
fn an_object_entry_parses_and_its_usage_sibling_is_no_block() {
    assert_eq!(
        blocks(br#"{"content":[{"type":"text","text":"hi"}],"usage":{"input_tokens":5}}"#),
        vec![Content::Text("hi".into())]
    );
}

#[test]
fn an_object_entry_without_usage_parses() {
    assert_eq!(
        blocks(br#"{"content":[{"type":"text","text":"quiet"}]}"#),
        vec![Content::Text("quiet".into())]
    );
}

#[test]
fn an_empty_entry_parses_in_either_shape() {
    assert_eq!(blocks(b"[]"), Vec::<Content>::new());
    assert_eq!(blocks(br#"{"content":[]}"#), Vec::<Content>::new());
}

#[test]
fn no_reported_counter_seals_no_usage_sibling() {
    let report = UsageReport::default();
    assert_eq!(sealed("", &report), json!({"content": []}));
}

#[test]
fn an_all_unreported_usage_event_reports_nothing() {
    // A provider that names no counter is not a provider reporting zero
    // (brazen's zero-vs-unknown rule): the entry stays usage-free.
    let mut report = UsageReport::default();
    report.fold(&Usage::default());
    assert_eq!(sealed("", &report), json!({"content": []}));
}

#[test]
fn counters_reported_across_two_events_seal_as_one_report() {
    // Anthropic's shape: `message_start` carries the input side, the
    // terminal `message_delta` the output side (§2.3 *Usage rides the entry*).
    let mut report = UsageReport::default();
    report.fold(&usage_of(Some(5), Some(0)));
    report.fold(&usage_of(None, Some(3)));
    assert_eq!(
        sealed(r#"{"type":"text","text":"hi"}"#, &report),
        json!({
            "content": [{"type": "text", "text": "hi"}],
            "usage": {"input_tokens": 5, "output_tokens": 3},
        })
    );
}

#[test]
fn a_reported_counter_is_never_superseded_by_an_unreported_one() {
    let mut report = UsageReport::default();
    report.fold(&usage_of(Some(7), Some(2)));
    report.fold(&Usage::default());
    assert_eq!(
        sealed("", &report)["usage"],
        json!({"input_tokens": 7, "output_tokens": 2})
    );
}

#[test]
fn a_counter_brazen_added_under_v1_rides_through_with_no_edit_here() {
    // The fold is over the SERIALIZED counter names, not a field list, so
    // `input_total_tokens` — brazen 0.0.10's resolved prompt total
    // (bl-d192) — reaches the committed entry without an edit in this
    // module. The compaction window trigger reads it back off exactly
    // these bytes (`prompt::compactor::checkpoint::usage`).
    let mut u = Usage::default();
    u.input_tokens = Some(1);
    u.cache_read_tokens = Some(900);
    u.input_total_tokens = Some(901);
    u.context_window = Some(200_000);
    let mut report = UsageReport::default();
    report.fold(&u);
    assert_eq!(
        sealed("", &report)["usage"],
        json!({"input_tokens": 1, "cache_read_tokens": 900,
               "input_total_tokens": 901, "context_window": 200_000})
    );
}

#[test]
fn cache_counters_ride_through_verbatim() {
    let mut u = Usage::default();
    u.input_tokens = Some(1);
    u.cache_read_tokens = Some(900);
    u.cache_write_tokens = Some(64);
    let mut report = UsageReport::default();
    report.fold(&u);
    assert_eq!(
        sealed("", &report)["usage"],
        json!({"input_tokens": 1, "cache_read_tokens": 900, "cache_write_tokens": 64})
    );
}
