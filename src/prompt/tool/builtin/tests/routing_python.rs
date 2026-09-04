//! Routing test for the `python` built-in
//! (`docs/DESIGN_CODE_EXECUTION.md` §2.2). The name is answered by
//! [`super::super::run`] and not by `run_with`: a program's toolset is
//! resolved from the whole binding (driver target, adapter target, stop
//! flag, tool injection), which only the production entry point holds.
//! What the module *does* with that is
//! `prompt::tool::builtin::python::tests`'; this pins the arm.

use super::super::Error;
use super::route;
use std::io::Cursor;

#[test]
fn python_routed_to_inner_module_and_its_errors_carried_through() {
    // The test process is not a tool subprocess, so the §3.3 contract's
    // `LITANY_TOOL_ID` is unset — the built-in's first refusal, reached
    // only by going through the dispatcher's `python` arm.
    let input = serde_json::json!({ "program": "print(1)" }).to_string();
    let mut stdin = Cursor::new(input.into_bytes());
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();
    let err = route("python", &mut stdin, &mut stdout, &mut stderr).unwrap_err();
    assert!(matches!(err, Error::Python(_)), "{err}");
    assert!(err.to_string().contains("LITANY_TOOL_ID"), "{err}");
}
