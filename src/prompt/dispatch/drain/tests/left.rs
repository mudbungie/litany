//! What the drain **leaves** and what it **reports** (ARCH §2.11, §2.6;
//! `docs/DESIGN_PROCEDURES.md` §3): an own child's result message stays
//! for the §6 interpreter, a foreign reply delivers, an absent inbox only
//! probes for strays, and the delivery says whether the operator spoke.
//! Split from [`super`] along that seam to hold the per-file line cap.

use super::*;

/// A result message's on-disk body (§2.6) from `sender`.
fn result_body(sender: &str) -> String {
    format!("---\nfrom: {sender}\nepitaph: final-response\nterminal_ref: abc123\n---\ndone")
}

#[test]
fn drain_leaves_an_own_childs_result_message_in_the_inbox_for_the_interpreter() {
    // A deposit carrying `terminal_ref:` *from this agent's own child* is
    // a result message (§2.6): a §6 lifecycle circumstance, not steering.
    let dir = TempDir::new().unwrap();
    let worktree = dir.path().join("wt");
    std::fs::create_dir_all(&worktree).unwrap();
    let inbox = dir.path().join("inbox/20260101-a1");
    let child = "20260101-a1-20260102-b2";
    deposit_file(
        &inbox,
        &format!("{child}-001.md"),
        &result_body(child),
        at(10),
    );

    let git = RecordGit::clean();
    drain(&worktree, &inbox, "20260101-a1", &git).unwrap();

    // No delivery: only the clean stray probe ran; the file stays put.
    let runs = git.runs.borrow();
    assert_eq!(runs.len(), 1);
    assert_eq!(runs[0], vec!["status", "--porcelain", "--", "messages"]);
    assert!(inbox.join(format!("{child}-001.md")).exists());
    assert!(!worktree.join(format!("messages/001-{child}.md")).exists());
}

#[test]
fn drain_delivers_a_reply_from_an_agent_this_one_never_dispatched() {
    // §2.6: the return — the transfer and the §6 bindings — is the
    // *dispatcher's* business. A sibling's reply carries the same
    // frontmatter but neither relationship, so it just delivers (§2.11).
    let dir = TempDir::new().unwrap();
    let worktree = dir.path().join("wt");
    std::fs::create_dir_all(&worktree).unwrap();
    let inbox = dir.path().join("inbox/20260101-a1-20260102-b2");
    let sibling = "20260101-a1-20260102-c3";
    deposit_file(
        &inbox,
        &format!("{sibling}-001.md"),
        &result_body(sibling),
        at(10),
    );

    let git = RecordGit::clean();
    let delivery = drain(&worktree, &inbox, "20260101-a1-20260102-b2", &git).unwrap();

    assert_eq!(delivery.delivered, 1);
    assert!(delivery.left.is_empty(), "nothing is held for a §6 binding");
    assert!(!inbox.join(format!("{sibling}-001.md")).exists());
    let landed = std::fs::read_to_string(worktree.join(format!("messages/001-{sibling}.md")));
    assert!(landed.unwrap().contains("epitaph: final-response"));
}

#[test]
fn drain_on_an_absent_inbox_only_probes_for_strays() {
    let dir = TempDir::new().unwrap();
    let worktree = dir.path().join("wt");
    std::fs::create_dir_all(&worktree).unwrap();
    let git = RecordGit::clean();
    drain(&worktree, &dir.path().join("inbox/nobody"), "c", &git).unwrap();
    // Clean stray probe, no inbox files: exactly the one status run.
    assert_eq!(git.runs.borrow().len(), 1);
}

#[test]
fn drain_reports_whether_the_operator_spoke() {
    // `docs/DESIGN_PROCEDURES.md` §3: the `user_message` circumstance is
    // "an operator message was delivered", once per drain, whatever else
    // arrived beside it — and a drain that delivered none says so.
    let dir = TempDir::new().unwrap();
    let worktree = dir.path().join("wt");
    std::fs::create_dir_all(&worktree).unwrap();
    let inbox = dir.path().join("inbox/agent");
    deposit_file(&inbox, "bob-001.md", "---\nfrom: bob\n---\nyo", at(10));
    let git = RecordGit::clean();
    assert!(
        !drain(&worktree, &inbox, "conv-7", &git)
            .unwrap()
            .operator_spoke
    );

    deposit_file(&inbox, "user-001.md", "---\nfrom: user\n---\nhi", at(20));
    deposit_file(&inbox, "bob-002.md", "---\nfrom: bob\n---\nyo", at(30));
    let git = RecordGit::clean();
    let delivery = drain(&worktree, &inbox, "conv-7", &git).unwrap();
    assert_eq!(delivery.delivered, 2);
    assert!(delivery.operator_spoke);
}
