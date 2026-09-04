//! The `search_history` routing arm (ARCH §3.3,
//! `docs/DESIGN_CONTEXT_ECONOMY.md` §4): the dispatcher hands the call
//! to the inner module and carries its decline back through
//! [`Error::SearchHistory`].

use super::super::{Error, run_with};
use super::{StubSender, StubSpawner, stub_env};
use std::io::Cursor;

#[test]
fn search_history_routed_to_inner_module() {
    // A fresh workspace has one config commit and no agent branch, so
    // the search runs and answers an empty listing — the happy path
    // through the dispatcher, exit 0.
    let (_h, ws) = crate::workspace::fixture::workspace();
    let input = serde_json::json!({ "pattern": "anything" }).to_string();
    let mut stdin = Cursor::new(input.into_bytes());
    let (mut stdout, mut stderr) = (Vec::new(), Vec::new());
    let code = run_with(
        "search_history",
        &mut stdin,
        &mut stdout,
        &mut stderr,
        &stub_env(&ws, "p1"),
        &StubSpawner,
        &StubSender,
    )
    .unwrap();
    assert_eq!(code, 0);
    assert!(stdout.is_empty(), "{stdout:?}");
}

#[test]
fn search_history_error_is_carried_through_dispatcher() {
    // Neither input — search_history::Error::Ambiguous via `#[from]`.
    let repo = tempfile::TempDir::new().unwrap();
    let mut stdin = Cursor::new(b"{}".to_vec());
    let (mut stdout, mut stderr) = (Vec::new(), Vec::new());
    let err = run_with(
        "search_history",
        &mut stdin,
        &mut stdout,
        &mut stderr,
        &stub_env(repo.path(), "p1"),
        &StubSpawner,
        &StubSender,
    )
    .unwrap_err();
    assert!(matches!(err, Error::SearchHistory(_)), "{err}");
    // `#[error(transparent)]`: the inner decline reaches the model verbatim.
    assert!(err.to_string().contains("exactly one of"), "{err}");
}
