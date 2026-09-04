//! The `{pattern}` answer (§4): what is searched, what is listed, and
//! how the newest five previews are bounded. Split from [`super`] for
//! the per-file line cap.

use super::*;

#[test]
fn a_pattern_lists_addresses_and_previews_the_entry_verbatim() {
    let (holder, repo) = workspace_repo();
    commit(
        &repo,
        "step 002",
        &[("messages/001-user.md", "the tin roof decision\n")],
        &[],
    );
    let answer = ask(holder.path(), &serde_json::json!({"pattern": "tin roof"}));
    let addrs = addresses(&answer);
    assert_eq!(addrs.len(), 1, "{answer}");
    assert!(addrs[0].ends_with(":messages/001-user.md"), "{answer}");
    // The preview is framed by the same address and carries the stored
    // bytes, trailing newline included — verbatim, not re-rendered.
    assert!(
        answer.contains(&format!(
            "<entry address=\"{}\">\nthe tin roof decision\n\n</entry>\n",
            addrs[0]
        )),
        "{answer}"
    );
    assert_eq!(
        g().run_capture(&repo, &["rev-parse", "HEAD"]).unwrap(),
        addrs[0].split(':').next().unwrap()
    );
}

#[test]
fn only_transcript_entries_are_searched_never_work_products() {
    let (holder, repo) = workspace_repo();
    commit(
        &repo,
        "step 002",
        &[
            ("notes/plan.md", "the tin roof decision\n"),
            ("summary/001.md", "roofing, generally\n"),
        ],
        &[],
    );
    // The work product carries the pattern and is not history (§4): the
    // pathspec is `messages` + `summary` and nothing else.
    assert_eq!(
        ask(holder.path(), &serde_json::json!({"pattern": "tin roof"})),
        ""
    );
    assert_eq!(
        addresses(&ask(
            holder.path(),
            &serde_json::json!({"pattern": "roofing"})
        ))
        .len(),
        1
    );
}

#[test]
fn no_hit_is_a_clean_empty_listing() {
    let (holder, _repo) = workspace_repo();
    assert_eq!(
        ask(holder.path(), &serde_json::json!({"pattern": "never said"})),
        ""
    );
}

#[test]
fn only_the_newest_five_hits_are_previewed() {
    let (holder, repo) = workspace_repo();
    for n in 1..=6 {
        commit(
            &repo,
            "step",
            &[(&format!("messages/00{n}-user.md"), "needle\n")],
            &[],
        );
    }
    let answer = ask(holder.path(), &serde_json::json!({"pattern": "needle"}));
    assert_eq!(addresses(&answer).len(), 6, "{answer}");
    assert_eq!(answer.matches("<entry address=").count(), PREVIEW_COUNT);
    // Newest first: the oldest hit is listed and not previewed.
    assert!(answer.contains("messages/001-user.md\n"), "{answer}");
    assert!(
        !answer.contains("<entry address=\"") || !answer.contains(":messages/001-user.md\">"),
        "{answer}"
    );
}

#[test]
fn an_oversize_preview_is_cut_and_the_marker_names_the_address() {
    let (holder, repo) = workspace_repo();
    let big = format!("needle\n{}\ntail marker\n", "x".repeat(20_000));
    commit(&repo, "step 002", &[("summary/001.md", big.as_str())], &[]);
    let answer = ask(holder.path(), &serde_json::json!({"pattern": "needle"}));
    let address = addresses(&answer)[0].to_string();
    assert!(
        answer.contains(&format!("full record: {address} ...]")),
        "{answer}"
    );
    assert!(answer.contains("entry truncated:"), "{answer}");
    // Head and tail both survive; the middle does not.
    assert!(answer.contains("needle\n"), "{answer}");
    assert!(answer.contains("tail marker\n"), "{answer}");
    assert!(answer.len() < big.len(), "{answer}");

    // The address recovers the entry whole — the recovery path the
    // marker advertises, byte for byte with what was committed.
    assert_eq!(
        ask(holder.path(), &serde_json::json!({"entry": address})),
        big
    );
}
