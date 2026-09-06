//! The reply debt (ARCH §2.6 *A reply is owed once*): a prompt is
//! answered once, and being answered asks nothing.

use super::*;
use tempfile::TempDir;

const PARENT: &str = "20260101-a1";
const SPECCER: &str = "20260101-a1-20260102-s1";
const BUILDER: &str = "20260101-a1-20260102-b2";
const CHILD_OF_BUILDER: &str = "20260101-a1-20260102-b2-20260103-w9";

/// The debt over a worktree: the one read plus the predicate, which is
/// how [`super::super::recipient`] asks it.
fn owed(worktree: &std::path::Path, agent_id: &str) -> Result<bool, crate::prompt::Error> {
    owes_a_reply(&read_entries(worktree)?, agent_id)
}

/// A worktree carrying the given `messages/` entries in order.
fn transcript(entries: &[(&str, &str)]) -> TempDir {
    let wt = TempDir::new().unwrap();
    std::fs::create_dir_all(wt.path().join("messages")).unwrap();
    for (name, body) in entries {
        std::fs::write(wt.path().join("messages").join(name), body).unwrap();
    }
    wt
}

/// A delivered prompt's on-disk body (§2.11 frontmatter).
fn prompt(from: &str) -> String {
    format!("---\nfrom: {from}\ndeposited_at: t\n---\ndo it\n")
}

/// A delivered result message: a prompt plus the two pinned §2.6 fields,
/// which is what makes it an answer rather than a question.
fn result(from: &str) -> String {
    format!(
        "---\nfrom: {from}\ndeposited_at: t\nepitaph: final-response\nterminal_ref: sha\n---\nok\n"
    )
}

/// A committed model-output entry that ends the step loop: no `tool_use`.
const TERMINAL: &str = r#"[{"type":"text","text":"Task complete."}]"#;
/// A committed model-output entry that does not: it calls a tool.
const TOOL_CALL: &str = r#"[{"type":"tool_use","id":"t1","name":"bash","input":{}}]"#;

#[test]
fn a_branch_that_has_never_answered_owes_its_first_reply() {
    // The general path with an empty window, not a bootstrap case: with
    // no previous terminal response there is nothing this reply could be
    // a repetition of. Covers the bare branch too.
    let wt = transcript(&[("001-20260101-a1.md", &prompt(PARENT))]);
    assert!(owed(wt.path(), BUILDER).unwrap());
    let bare = TempDir::new().unwrap();
    assert!(owed(bare.path(), BUILDER).unwrap());
}

#[test]
fn a_prompt_delivered_since_the_last_answer_is_owed_one() {
    let wt = transcript(&[
        ("001-20260101-a1.md", &prompt(PARENT)),
        ("002-claude-sonnet-5.json", TERMINAL),
        ("003-20260101-a1-20260102-s1.md", &prompt(SPECCER)),
        ("004-claude-sonnet-5.json", TERMINAL),
    ]);
    assert!(owed(wt.path(), BUILDER).unwrap());
}

#[test]
fn a_foreign_reply_asks_nothing_so_the_exchange_terminates() {
    // THE PIN (bl-82d8). Builder answered Speccer's spec at 002; Speccer
    // then answered *that* at 003. Its answer is not a question, and the
    // spec it re-reads at 001 was answered already — so this terminal
    // event owes nobody, deposits nothing, and the ping-pong has a floor.
    // Before this, 001 stayed the standing prompt forever and every
    // terminal response was re-delivered to the peer: 333 transcript rows
    // and 13.4M tokens out of one four-step exchange.
    let wt = transcript(&[
        ("001-20260101-a1-20260102-s1.md", &prompt(SPECCER)),
        ("002-claude-sonnet-5.json", TERMINAL),
        ("003-20260101-a1-20260102-s1.md", &result(SPECCER)),
        ("004-claude-sonnet-5.json", TERMINAL),
    ]);
    assert!(!owed(wt.path(), BUILDER).unwrap());
}

#[test]
fn an_own_childs_return_keeps_the_answer_owed() {
    // A dispatcher parked on its children is not done answering: the
    // child's work-product transfer lands on *this* branch (§2.6), so
    // the commissioned work is arriving toward the standing prompt. The
    // one arm that separates a coordinator from the ping-pong above.
    let wt = transcript(&[
        ("001-20260101-a1.md", &prompt(PARENT)),
        ("002-claude-sonnet-5.json", TERMINAL),
        (
            "003-20260101-a1-20260102-b2-20260103-w9.md",
            &result(CHILD_OF_BUILDER),
        ),
        ("004-claude-sonnet-5.json", TERMINAL),
    ]);
    assert!(owed(wt.path(), BUILDER).unwrap());
}

#[test]
fn a_self_note_asks_nothing() {
    // §2.11: a note to oneself is answered by one's own next step, which
    // has already happened.
    let wt = transcript(&[
        ("001-20260101-a1.md", &prompt(PARENT)),
        ("002-claude-sonnet-5.json", TERMINAL),
        ("003-20260101-a1-20260102-b2.md", &prompt(BUILDER)),
        ("004-claude-sonnet-5.json", TERMINAL),
    ]);
    assert!(!owed(wt.path(), BUILDER).unwrap());
}

#[test]
fn the_window_opens_at_the_previous_terminal_and_skips_tool_steps() {
    // A `tool_use` entry is not a terminal response and a `tool` result
    // entry is not model output, so neither closes the window: the
    // prompt at 003 is still inside it and still owed an answer.
    let wt = transcript(&[
        ("001-20260101-a1.md", &prompt(PARENT)),
        ("002-claude-sonnet-5.json", TERMINAL),
        ("003-20260101-a1-20260102-s1.md", &prompt(SPECCER)),
        ("004-claude-sonnet-5.json", TOOL_CALL),
        ("005-tool.json", "[]"),
        ("006-claude-sonnet-5.json", TERMINAL),
    ]);
    assert!(owed(wt.path(), BUILDER).unwrap());
}

#[test]
fn a_prompt_before_the_previous_answer_is_outside_the_window() {
    // The same transcript with nothing delivered since 002: the prompt at
    // 001 was answered there, and answering it again is not a new answer.
    let wt = transcript(&[
        ("001-20260101-a1.md", &prompt(PARENT)),
        ("002-claude-sonnet-5.json", TERMINAL),
        ("003-claude-sonnet-5.json", TERMINAL),
    ]);
    assert!(!owed(wt.path(), BUILDER).unwrap());
}

#[test]
fn an_unparseable_name_is_not_an_entry() {
    // Neither `NNN-<origin>.md` nor `NNN-<origin>.json` — a stray, not a
    // transcript entry, so it neither closes a window nor owes a reply.
    let wt = transcript(&[
        ("noseq.md", "x"),
        ("abc-user.md", "x"),
        ("001-.md", "x"),
        ("002-claude-sonnet-5.txt", TERMINAL),
    ]);
    assert!(owed(wt.path(), BUILDER).unwrap());
}

#[test]
fn an_unreadable_entry_surfaces_rather_than_answering_nobody() {
    // The read is the derivation's evidence (§2.6): a directory where an
    // entry should be is an I/O error, never a silent "owes nothing".
    let wt = transcript(&[("001-20260101-a1.md", &prompt(PARENT))]);
    std::fs::create_dir(wt.path().join("messages/002-claude-sonnet-5.json")).unwrap();
    assert!(owed(wt.path(), BUILDER).is_err());
    let wt2 = transcript(&[
        ("002-claude-sonnet-5.json", TERMINAL),
        ("003-claude-sonnet-5.json", TERMINAL),
    ]);
    std::fs::create_dir(wt2.path().join("messages/004-20260101-a1.md")).unwrap();
    assert!(owed(wt2.path(), BUILDER).is_err());
}

#[test]
fn a_messages_path_that_is_not_a_directory_surfaces() {
    let wt = TempDir::new().unwrap();
    std::fs::write(wt.path().join("messages"), "not a directory").unwrap();
    assert!(owed(wt.path(), BUILDER).is_err());
}
