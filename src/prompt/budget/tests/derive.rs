//! Derivation tests: [`spend`], [`wall_seconds`], [`depth`] over
//! on-disk step trees (ARCH §6/§8).

use super::super::derive::{depth, spend, wall_seconds};
use super::{repo, seg, usage_line, usage_line_with_total, write_meta, write_response};

#[test]
fn spend_sums_every_segment_of_every_step() {
    let r = repo();
    // Step 1: two segments (a failed attempt + a retry) — both billed
    // (ARCH §6 "Every attempt segment counts").
    let s1 = format!(
        "{}{}",
        seg(&usage_line(Some(5), Some(3), None, None)),
        seg(&usage_line(Some(10), Some(2), None, None))
    );
    write_response(r.path(), "conv", 1, &s1);
    // Step 2: one segment.
    write_response(
        r.path(),
        "conv",
        2,
        &seg(&usage_line(Some(4), Some(1), None, None)),
    );
    // (5+3) + (10+2) + (4+1) = 25
    assert_eq!(spend(r.path(), "conv"), 25);
}

#[test]
fn spend_includes_descendant_subagents_not_unrelated_convs() {
    let r = repo();
    write_response(
        r.path(),
        "conv",
        1,
        &seg(&usage_line(Some(100), None, None, None)),
    );
    write_response(
        r.path(),
        "conv-child",
        1,
        &seg(&usage_line(Some(30), None, None, None)),
    );
    // A conv that merely shares a prefix without the `-` boundary, and an
    // unrelated conv, are both excluded.
    write_response(
        r.path(),
        "convX",
        1,
        &seg(&usage_line(Some(7), None, None, None)),
    );
    write_response(
        r.path(),
        "other",
        1,
        &seg(&usage_line(Some(999), None, None, None)),
    );
    assert_eq!(spend(r.path(), "conv"), 130);
}

#[test]
fn spend_none_counter_is_zero_and_cache_counters_count() {
    let r = repo();
    // input None → 0, so the cache counters are the whole prompt
    // (max(0, 2+1) = 3) and output adds beside it.
    let body = seg(&usage_line(None, Some(7), Some(2), Some(1)));
    write_response(r.path(), "conv", 1, &body);
    assert_eq!(spend(r.path(), "conv"), 10);
}

#[test]
fn spend_takes_the_served_input_total_over_the_fallback_fold() {
    // Since brazen 0.0.10 every decoder seals `input_total_tokens` — the
    // whole prompt, cached slices included. Anthropic's three prompt
    // counters are disjoint, so the served total is their sum (92,200)
    // while the fallback fold's max() would report only 91,000: the two
    // answers differ, and the served one wins (ARCH §6).
    let r = repo();
    let body = seg(&usage_line_with_total(
        Some(1_200),
        Some(50),
        Some(90_000),
        Some(1_000),
        Some(92_200),
    ));
    write_response(r.path(), "conv", 1, &body);
    assert_eq!(spend(r.path(), "conv"), 92_250);
}

#[test]
fn spend_bills_a_contained_cached_slice_once() {
    // A record written by a pre-0.0.10 `bz` carries no
    // `input_total_tokens`, so the fallback fold answers. OpenAI-shaped /
    // Google-shaped providers report a prompt counter that CONTAINS the
    // cached one (`prompt_tokens` ⊇ `cached_tokens`, `promptTokenCount` ⊇
    // `cachedContentTokenCount`). Summing the four would bill the cached
    // slice twice — here 185,336 for a 93,556-token prompt (ARCH §6 "The
    // cached slice is billed once").
    let r = repo();
    let body = seg(&usage_line(Some(93_556), Some(132), Some(91_648), None));
    write_response(r.path(), "conv", 1, &body);
    assert_eq!(spend(r.path(), "conv"), 93_688);
}

#[test]
fn spend_counts_disjoint_cache_counters_beside_a_smaller_prompt() {
    // The same pre-0.0.10 record, Anthropic-shaped: its three prompt
    // counters are disjoint slices, and the uncached remainder is
    // typically the small one — so the fallback fold takes
    // cache_read + cache_write and the result is a floor, never an
    // over-statement (ARCH §6).
    let r = repo();
    let body = seg(&usage_line(
        Some(1_200),
        Some(50),
        Some(90_000),
        Some(1_000),
    ));
    write_response(r.path(), "conv", 1, &body);
    assert_eq!(spend(r.path(), "conv"), 91_050);
}

#[test]
fn spend_zero_when_no_steps_dir() {
    let r = repo();
    assert_eq!(spend(r.path(), "conv"), 0);
}

#[test]
fn spend_tolerates_missing_response_nonnumeric_dir_and_malformed_lines() {
    let r = repo();
    // Step dir exists but no response.json → 0.
    std::fs::create_dir_all(r.path().join("steps/conv/001")).unwrap();
    // Non-numeric step subdir (e.g. a `tools/` sibling) is skipped.
    std::fs::create_dir_all(r.path().join("steps/conv/tools")).unwrap();
    // A malformed line + a good usage line in step 002.
    let body = format!(
        "not json\n{}\n{{\"type\":\"end\"}}\n",
        usage_line(Some(8), None, None, None)
    );
    write_response(r.path(), "conv", 2, &body);
    assert_eq!(spend(r.path(), "conv"), 8);
}

#[test]
fn spend_tolerates_a_conv_entry_that_is_a_file() {
    let r = repo();
    write_response(
        r.path(),
        "conv",
        1,
        &seg(&usage_line(Some(6), None, None, None)),
    );
    // A stray file matching the descent prefix: read_dir on it errors → 0.
    std::fs::write(r.path().join("steps/conv-stray"), b"x").unwrap();
    assert_eq!(spend(r.path(), "conv"), 6);
}

#[test]
fn wall_sums_step_spans_including_backoff_across_descent() {
    let r = repo();
    // Each span already includes the step's backoff sleeps (§4.4 fd held
    // open across attempts + backoff), so summing spans is "wall is wall".
    write_meta(
        r.path(),
        "conv",
        1,
        "2026-01-01T00:00:00Z",
        "2026-01-01T00:00:10Z",
    );
    write_meta(
        r.path(),
        "conv",
        2,
        "2026-01-01T00:01:00Z",
        "2026-01-01T00:01:05Z",
    );
    write_meta(
        r.path(),
        "conv-child",
        1,
        "2026-01-01T00:02:00Z",
        "2026-01-01T00:02:03Z",
    );
    assert_eq!(wall_seconds(r.path(), "conv"), 18);
}

#[test]
fn wall_tolerates_missing_malformed_unparseable_and_backwards_spans() {
    let r = repo();
    // No meta.json → 0.
    std::fs::create_dir_all(r.path().join("steps/conv/001")).unwrap();
    // Malformed meta.json → 0.
    let d2 = r.path().join("steps/conv/002");
    std::fs::create_dir_all(&d2).unwrap();
    std::fs::write(d2.join("meta.json"), b"{ not json").unwrap();
    // Unparseable timestamps → 0.
    write_meta(r.path(), "conv", 3, "iso-1", "iso-2");
    // ended before started → 0.
    write_meta(
        r.path(),
        "conv",
        4,
        "2026-01-01T00:00:10Z",
        "2026-01-01T00:00:00Z",
    );
    // one good span
    write_meta(
        r.path(),
        "conv",
        5,
        "2026-01-01T00:00:00Z",
        "2026-01-01T00:00:07Z",
    );
    assert_eq!(wall_seconds(r.path(), "conv"), 7);
}

#[test]
fn depth_counts_dispatch_levels_from_hyphenated_descent() {
    // Real conv-ids: hyphen-free compact ts + hyphen-free short id.
    assert_eq!(depth("20260422T065432Z-a1b2c3d4"), 0);
    assert_eq!(
        depth("20260422T065432Z-a1b2c3d4-20260422T065500Z-e5f6a7b8"),
        1
    );
    assert_eq!(
        depth("20260422T065432Z-a1b2c3d4-20260422T065500Z-e5f6a7b8-20260422T070000Z-deadbeef"),
        2
    );
}
