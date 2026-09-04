//! Command-surface tests (ARCH §3.4). [`surface`] pins the clap argv
//! shape and the binding types ([`Outcome`], [`Error`], [`prelude`]);
//! [`verbs`] drives each [`Command`] entry against a constructed [`Fx`];
//! [`agent_id`] pins the per-verb agent-id guard (§2.3), and
//! [`naming`] the agent-name fact across the surface (§2.3, §2.11).
//! The provider-driven happy paths (`prompt` product, the `advance`
//! successor `exec`, `prime` success) are pinned by the `tests/*_cli.rs`
//! end-to-end binary tests; here the cheap early-error paths cover the
//! surface-layer wiring.

use super::*;
use crate::test_support::with_litany_home;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;

mod agent_id;
mod cwd;
mod dispatching;
mod invoking;
mod invoking_faults;
mod invoking_gates;
mod naming;
mod pins;
mod proposing;
mod retargeting;
mod skilling;
mod surface;
mod verbs;
mod verbs_more;
mod workflowing;
mod workflowing_declines;

/// A no-op `$EDITOR` hand-off.
fn noop_editor(_: &Path) -> std::io::Result<()> {
    Ok(())
}

/// An `$EDITOR` hand-off that writes a distinct `providers.yaml` so the
/// authoring pass (`litany config`) has a non-empty commit to land.
fn writing_editor(dir: &Path) -> std::io::Result<()> {
    std::fs::write(dir.join("providers.yaml"), "roles: {}\n")
}

/// Assert a verb failure renders the uniform `litany <prefix>: …` shape.
fn assert_prefixed(err: Error, prefix: &str) {
    let s = err.to_string();
    assert!(s.starts_with(&format!("litany {prefix}: ")), "{s}");
}

/// Build an [`Fx`] around scratch stdio plus `editor`, run `f`, and hand
/// back its result with the captured `(stdout, stderr)`.
// `#[rustfmt::skip]` keeps the `Fx` literal on one line: exploded across
// field lines, tarpaulin's llvm engine mis-attributes the `&mut` field
// lines as uncovered (the same known multi-line quirk `tool::builtin`
// documents); every field here runs on every call.
#[rustfmt::skip]
fn with_fx<R>(
    driver_target: &str,
    stdin: &[u8],
    editor: &dyn Fn(&Path) -> std::io::Result<()>,
    f: impl FnOnce(&mut Fx) -> R,
) -> (R, Vec<u8>, Vec<u8>) {
    let mut stdin_ref = stdin;
    let mut stdout: Vec<u8> = Vec::new();
    let mut stderr: Vec<u8> = Vec::new();
    let stop = AtomicBool::new(false);
    let mut fx = Fx { driver_target: PathBuf::from(driver_target), adapter_target: None, editor, tool_stdin: &mut stdin_ref, tool_stdout: &mut stdout, tool_stderr: &mut stderr, stop: &stop, tool_injection: None };
    let r = f(&mut fx);
    drop(fx);
    (r, stdout, stderr)
}
