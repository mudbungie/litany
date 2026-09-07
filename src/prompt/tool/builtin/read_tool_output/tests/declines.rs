//! Every way the page read declines (ARCH §3.3 *Paging a cut capture*).
//!
//! Split from [`super`] at the per-file cap. Each assertion reads the
//! decline the model reads: per §3.3 stderr concatenates after stdout
//! into `tool_result.content` on a non-zero exit, so a decline that
//! does not say what to correct is a step the agent cannot recover
//! from on its own.

use super::super::{Error, run};
use super::{AGENT, StubEnv, address, body, read, record_dir, workspace};
use crate::prompt::tool::{ENV_CONV_BRANCH, ENV_CONV_REPO};
use std::collections::HashMap;
use std::ffi::OsString;
use std::io::Cursor;
use tempfile::TempDir;

/// The domain bound: an address naming another agent is refused **by
/// name**, before anything is opened. The refusal states both ids so a
/// model that mis-copied one can see which half is wrong.
#[test]
fn a_foreign_agent_address_is_refused_by_name() {
    let ws = workspace("secrets", "");
    let err = read(ws.path(), "someone-else", &body(&address("stdout", 0))).expect_err("refused");
    assert!(matches!(err, Error::ForeignAgent { .. }));
    assert_eq!(
        err.to_string(),
        format!(
            "address names agent {AGENT:?}, and you are \"someone-else\": \
             an agent may page only its own tool captures"
        )
    );
}

/// Every shape that is not the grammar declines by stating the grammar
/// — including the traversal attempts, which never reach the disk.
#[test]
fn a_malformed_address_states_the_grammar() {
    let ws = workspace("abc", "");
    for bad in [
        "steps/root-child/001/tools/tu_1/output.json",
        "steps/root-child/001/tools/tu_1/output.json#stdout",
        "steps/root-child/001/tools/tu_1/output.json#stdout@x",
        "steps/root-child/001/tools/tu_1/output.json#stdlog@0",
        "steps/root-child/001/tools/tu_1/input.json#stdout@0",
        "logs/root-child/001/tools/tu_1/output.json#stdout@0",
        "steps/root-child/001/records/tu_1/output.json#stdout@0",
        "steps/root-child/00x/tools/tu_1/output.json#stdout@0",
        "steps/root-child//tools/tu_1/output.json#stdout@0",
        "steps/../001/tools/tu_1/output.json#stdout@0",
        "steps/root-child/001/tools/./output.json#stdout@0",
        "/steps/root-child/001/tools/tu_1/output.json#stdout@0",
        "steps/root-child/001/tools/tu_1/extra/output.json#stdout@0",
    ] {
        let err = read(ws.path(), AGENT, &body(bad)).expect_err("refused");
        assert!(matches!(err, Error::Malformed { .. }), "{bad}: {err}");
        assert!(err.to_string().contains("byte offset>"), "{bad}");
    }
}

/// A record that is not there — swept by retention, or a call that never
/// landed one — is a decline naming the path, not a panic.
#[test]
fn an_absent_capture_declines() {
    let ws = TempDir::new().expect("tempdir");
    let err = read(ws.path(), AGENT, &body(&address("stdout", 0))).expect_err("refused");
    assert!(matches!(err, Error::NoRecord { .. }));
    assert!(err.to_string().starts_with("no capture at "), "{err}");
}

/// A record that is not the shape [`ToolOutputRecord`] pins declines as
/// unreadable, distinctly from an absent one.
#[test]
fn an_unparseable_capture_declines() {
    let ws = TempDir::new().expect("tempdir");
    let dir = ws.path().join(record_dir());
    std::fs::create_dir_all(&dir).expect("mkdir");
    std::fs::write(dir.join("output.json"), b"not json").expect("write");
    let err = read(ws.path(), AGENT, &body(&address("stdout", 0))).expect_err("refused");
    assert!(matches!(err, Error::Unreadable { .. }));
    assert!(err.to_string().contains("did not parse"), "{err}");
}

/// Past the end states the stream's true size, so the next read is a
/// correction rather than another guess.
#[test]
fn an_offset_past_the_end_states_the_total() {
    let ws = workspace("abc", "");
    let err = read(ws.path(), AGENT, &body(&address("stdout", 9))).expect_err("refused");
    assert_eq!(
        err.to_string(),
        "offset 9 is past the end of stdout (3 bytes)"
    );
}

/// The input is one field and no others: a `stream:` or `offset:`
/// beside the address would be a second home for what it already says.
#[test]
fn a_second_field_is_refused() {
    let ws = workspace("abc", "");
    let input = format!(
        "{{\"address\": \"{}\", \"offset\": 4}}",
        address("stdout", 0)
    );
    let err = read(ws.path(), AGENT, &input).expect_err("refused");
    assert!(matches!(err, Error::InvalidJson(_)));
    assert!(err.to_string().starts_with("invalid input JSON: "), "{err}");
}

/// Neither env var is inferred: the harness sets both (§3.3), and a tool
/// run without them declines rather than guessing whose capture to read.
#[test]
fn a_missing_env_var_declines_by_name() {
    let ws = workspace("abc", "");
    for absent in [ENV_CONV_REPO, ENV_CONV_BRANCH] {
        let mut m = HashMap::new();
        m.insert(ENV_CONV_REPO, ws.path().as_os_str().to_owned());
        m.insert(ENV_CONV_BRANCH, OsString::from(AGENT));
        m.remove(absent);
        let mut stdin = Cursor::new(body(&address("stdout", 0)).into_bytes());
        let err =
            run(&mut stdin, &mut Vec::new(), &mut Vec::new(), &StubEnv(m)).expect_err("refused");
        assert!(matches!(err, Error::MissingEnv(name) if name == absent));
        assert!(err.to_string().contains(absent), "{err}");
    }
}
