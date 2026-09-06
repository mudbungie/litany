//! The `remember` routing arm (`docs/DESIGN_CONTEXT_ECONOMY.md` §3): the
//! dispatcher hands the call to the inner module — which resolves the
//! real git for itself — and carries its decline back through
//! [`Error::Remember`].

use super::super::{Error, run_with};
use super::{StubEnv, StubSender, StubSpawner};
use std::io::Cursor;

/// The §3.3 contract plus the root the fixture primed its pools into,
/// which is what the inner module resolves the data root from.
fn env(home: &std::path::Path, ws: &std::path::Path, branch: &str) -> StubEnv {
    let mut m = std::collections::HashMap::new();
    m.insert(
        crate::prompt::tool::ENV_CONV_REPO,
        ws.as_os_str().to_owned(),
    );
    m.insert(
        crate::prompt::tool::ENV_CONV_BRANCH,
        std::ffi::OsString::from(branch),
    );
    m.insert(
        super::super::harness::ENV_LITANY_HOME,
        home.as_os_str().to_owned(),
    );
    StubEnv(m)
}

#[test]
fn remember_routed_to_inner_module() {
    let (holder, ws) = crate::workspace::fixture::workspace();
    crate::workspace::fixture::spawn_root(&ws, "p1");
    let input = serde_json::json!({ "fact": "the box has no network" }).to_string();
    let mut stdin = Cursor::new(input.into_bytes());
    let (mut stdout, mut stderr) = (Vec::new(), Vec::new());
    let code = run_with(
        "remember",
        &mut stdin,
        &mut stdout,
        &mut stderr,
        &env(&holder.path().join("data"), &ws, "p1"),
        &StubSpawner,
        &StubSender,
    )
    .unwrap();
    assert_eq!(code, 0);
    let payload: serde_json::Value = serde_json::from_slice(&stdout).unwrap();
    assert_eq!(payload["status"], "proposed");
    assert_eq!(payload["proposal"], "p1");
}

#[test]
fn remember_error_is_carried_through_dispatcher() {
    // An empty fact — remember::Error::Blank via `#[from]`, before any
    // ref is read.
    let (holder, ws) = crate::workspace::fixture::workspace();
    let input = serde_json::json!({ "fact": "  " }).to_string();
    let mut stdin = Cursor::new(input.into_bytes());
    let (mut stdout, mut stderr) = (Vec::new(), Vec::new());
    let err = run_with(
        "remember",
        &mut stdin,
        &mut stdout,
        &mut stderr,
        &env(&holder.path().join("data"), &ws, "p1"),
        &StubSpawner,
        &StubSender,
    )
    .unwrap_err();
    assert!(matches!(err, Error::Remember(_)), "{err}");
}
