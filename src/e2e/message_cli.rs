//! End-to-end subprocess tests for `litany message`'s recipient guards:
//! the agent id is one path component (ARCH §2.3) and the recipient
//! exists (§2.11 — "a message is content addressed to an *existing*
//! agent"). Both are declines the real binary must make before it
//! writes anything, so they are pinned through the process boundary
//! rather than only in-process.

use crate::prompt::inbox::{inbox_dir, try_acquire};
use crate::template::{GitRunner, RealGit};
use crate::test_support::litany_binary;
use crate::workspace::{agent_name, fixture};
use std::path::Path;
use std::process::Command;

/// Fork a root agent and land the name fact on it — the on-disk shape a
/// `litany prompt --name` / `litany dispatch --name` leaves behind
/// (ARCH §2.3), built without a provider.
fn named_root(ws: &Path, id: &str, name: &str) {
    let git = RealGit::new();
    let wt = fixture::spawn_root(ws, id);
    agent_name::settle(&wt, Some(name), &git).unwrap();
    git.run(&wt, &["commit", "-m", "settle name"]).unwrap();
}

/// Run `litany message <ws> <agent> <content>` and hand back
/// `(success, stderr)`.
fn message(ws: &Path, agent: &str, content: &str) -> (bool, String) {
    let out = Command::new(litany_binary())
        .arg("message")
        .arg(ws)
        .arg(agent)
        .arg(content)
        .output()
        .expect("spawn litany message");
    (
        out.status.success(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

#[test]
fn an_escaping_agent_id_is_declined_and_writes_nothing_outside_the_workspace() {
    let (holder, ws) = fixture::workspace();
    let (ok, stderr) = message(&ws, "../../victim/pwned", "hello");
    assert!(!ok, "an escaping id must exit non-zero");
    assert!(stderr.contains("litany message: agent id"), "{stderr}");
    // `<ws>/inbox/../../victim` is `<holder>/victim`.
    assert!(
        !holder.path().join("victim").exists(),
        "nothing is written outside the workspace"
    );
}

#[test]
fn a_recipient_with_no_branch_is_declined_rather_than_silently_deposited() {
    let (_h, ws) = fixture::workspace();
    let (ok, stderr) = message(&ws, "20260101-a1", "hello");
    assert!(!ok, "an unknown recipient must exit non-zero");
    assert!(stderr.contains("existing agent"), "{stderr}");
    assert!(
        !inbox_dir(&ws, "20260101-a1").exists(),
        "the decline creates no inbox directory"
    );
}

#[test]
fn an_existing_recipient_still_receives_its_deposit() {
    let (_h, ws) = fixture::workspace();
    fixture::spawn_root(&ws, "20260101-a1");
    // Hold the executor lease so the post-deposit probe reads Busy and
    // no real driver is spawned into the tempdir (§2.11).
    let _held = try_acquire(&inbox_dir(&ws, "20260101-a1"))
        .unwrap()
        .expect("free lease");
    let (ok, stderr) = message(&ws, "20260101-a1", "hello");
    assert!(ok, "{stderr}");
    let deposits = std::fs::read_dir(inbox_dir(&ws, "20260101-a1"))
        .unwrap()
        .flatten()
        .filter(|e| e.file_name().to_string_lossy().ends_with(".md"))
        .count();
    assert_eq!(deposits, 1, "exactly one deposit landed");
}

#[test]
fn a_recipient_addressed_by_its_display_name_receives_the_deposit() {
    // The yog repro (bl-c8ed): the display name every operator surface
    // speaks now resolves through the real binary, not just an id.
    let (_h, ws) = fixture::workspace();
    named_root(&ws, "20260101T000000Z-aaaaaaaa", "pale-otter");
    let _held = try_acquire(&inbox_dir(&ws, "20260101T000000Z-aaaaaaaa"))
        .unwrap()
        .expect("free lease");
    let (ok, stderr) = message(&ws, "pale-otter", "hello");
    assert!(ok, "{stderr}");
    let deposits = std::fs::read_dir(inbox_dir(&ws, "20260101T000000Z-aaaaaaaa"))
        .unwrap()
        .flatten()
        .count();
    assert_eq!(deposits, 1, "the name addressed the agent's own inbox");
    assert!(
        !inbox_dir(&ws, "pale-otter").exists(),
        "the name is never itself a namespace key"
    );
}

#[test]
fn a_name_two_living_agents_wear_is_refused_with_its_candidates() {
    let (_h, ws) = fixture::workspace();
    named_root(&ws, "20260101T000000Z-aaaaaaaa", "pale-otter");
    named_root(&ws, "20260102T000000Z-bbbbbbbb", "pale-otter");
    let (ok, stderr) = message(&ws, "pale-otter", "hello");
    assert!(!ok, "an ambiguous name must exit non-zero");
    assert!(stderr.contains("ambiguous"), "{stderr}");
    assert!(stderr.contains("20260101T000000Z-aaaaaaaa"), "{stderr}");
    assert!(stderr.contains("20260102T000000Z-bbbbbbbb"), "{stderr}");
    assert!(
        !inbox_dir(&ws, "20260101T000000Z-aaaaaaaa").exists(),
        "an ambiguous needle deposits nothing"
    );
}
