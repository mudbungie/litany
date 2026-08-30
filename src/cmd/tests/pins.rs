//! `--pin` on the two start verbs (ARCH §2.5 caller-supplied pinned
//! documents): exact CLI parity — `prompt` and `dispatch` run the same
//! `pinned_doc::load` ahead of everything the verb would otherwise do,
//! so a refused pin fails in the verb's own voice and leaves no branch,
//! ref or inbox behind. The happy landings are pinned by
//! `prompt::tests::pinned` (root) and `dispatch_cli::tests` (child);
//! here the surface-layer wiring and the fail-first ordering.

use super::{assert_prefixed, noop_editor, with_fx};
use crate::cmd::{dispatch, prompt};
use crate::workspace::fixture;

/// No agent branch, worktree or inbox exists after a refused pin — the
/// refusal preceded the fork.
fn assert_untouched(ws: &std::path::Path) {
    let count = std::fs::read_dir(ws.join(crate::workspace::AGENTS_DIR))
        .map(|d| d.count())
        .unwrap_or(0);
    assert_eq!(count, 0, "a refused pin left agent debris");
}

#[test]
fn prompt_declines_a_malformed_pin_in_its_own_voice() {
    let (_h, ws) = fixture::workspace();
    let (r, ..) = with_fx("litany", b"", &noop_editor, |fx| {
        prompt::run(
            prompt::Args {
                repo: ws.clone(),
                message: "hi".into(),
                from: None,
                config: None,
                name: None,
                pin: vec!["no-equals-here".into()],
                cwd: None,
            },
            fx,
        )
    });
    let err = r.unwrap_err();
    assert!(err.to_string().contains("<dest>=<source-path>"), "{err}");
    assert_prefixed(err, "prompt");
    assert_untouched(&ws);
}

#[test]
fn dispatch_declines_a_reserved_pin_destination_in_its_own_voice() {
    let (_h, ws) = fixture::workspace();
    fixture::spawn_root(&ws, "20260101-p1");
    let src = ws.join("pin-src.md");
    std::fs::write(&src, b"content").unwrap();
    let (r, ..) = with_fx("litany", b"", &noop_editor, |fx| {
        dispatch::run(
            dispatch::Args {
                role: "worker".into(),
                repo: ws.clone(),
                branch: "20260101-p1".into(),
                goal: Some("go".into()),
                from: None,
                name: None,
                pin: vec![format!("soul.md={}", src.display())],
                cwd: None,
            },
            fx,
        )
    });
    let err = r.unwrap_err();
    assert!(err.to_string().contains("harness-owned"), "{err}");
    assert_prefixed(err, "dispatch worker");
    // The parent's own worktree is the only agents/ entry — no child.
    let count = std::fs::read_dir(ws.join(crate::workspace::AGENTS_DIR))
        .unwrap()
        .count();
    assert_eq!(count, 1, "a refused pin left a child behind");
}
