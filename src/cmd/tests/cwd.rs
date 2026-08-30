//! `--cwd` on the two creation verbs (ARCH §3.3 *Working directory*):
//! exact CLI parity — `prompt` and `dispatch` run the mark's own
//! validation (`workspace::cwd::resolve`, the `cd` built-in's rules)
//! ahead of everything the verb would otherwise do, so a refused
//! directory fails in the verb's own voice and leaves no branch, ref or
//! inbox behind (§2.5). The landings are pinned by
//! `prompt::tests::cwd_seed` (root) and `child_dispatch::tests::cwd`
//! (child).

use super::{assert_prefixed, noop_editor, with_fx};
use crate::cmd::{dispatch, prompt};
use crate::workspace::fixture;

#[test]
fn prompt_declines_a_cwd_that_names_nothing_in_its_own_voice() {
    let (_h, ws) = fixture::workspace();
    let (r, ..) = with_fx("litany", b"", &noop_editor, |fx| {
        prompt::run(
            prompt::Args {
                repo: ws.clone(),
                message: "hi".into(),
                from: None,
                config: None,
                name: None,
                pin: vec![],
                cwd: Some("/no/such/place/at/all".into()),
            },
            fx,
        )
    });
    let err = r.unwrap_err();
    assert!(err.to_string().contains("no such directory"), "{err}");
    assert_prefixed(err, "prompt");
    // The refusal preceded the fork: no agent branch or worktree exists.
    let count = std::fs::read_dir(ws.join(crate::workspace::AGENTS_DIR))
        .map(|d| d.count())
        .unwrap_or(0);
    assert_eq!(count, 0, "a refused --cwd left agent debris");
}

#[test]
fn dispatch_declines_a_cwd_that_is_not_a_directory_in_its_own_voice() {
    let (_h, ws) = fixture::workspace();
    fixture::spawn_root(&ws, "20260101-p1");
    let file = ws.join("not-a-dir");
    std::fs::write(&file, b"x").unwrap();
    let (r, ..) = with_fx("litany", b"", &noop_editor, |fx| {
        dispatch::run(
            dispatch::Args {
                role: "worker".into(),
                repo: ws.clone(),
                branch: "20260101-p1".into(),
                goal: Some("go".into()),
                from: None,
                name: None,
                pin: vec![],
                cwd: Some(file.clone()),
            },
            fx,
        )
    });
    let err = r.unwrap_err();
    assert!(err.to_string().contains("is not a directory"), "{err}");
    assert_prefixed(err, "dispatch worker");
    // The parent's own worktree is the only agents/ entry — no child.
    let count = std::fs::read_dir(ws.join(crate::workspace::AGENTS_DIR))
        .unwrap()
        .count();
    assert_eq!(count, 1, "a refused --cwd left a child behind");
}
