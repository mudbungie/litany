//! The agent-name fact across the command surface (ARCH §2.3, §2.11):
//! `--name` at the two creation verbs, uniqueness refused where the fact
//! lives, and `message` resolving id-or-unique-name.
//!
//! The yog repro these close (bl-c8ed): an agent told to message a peer
//! by the display name every operator surface speaks failed, because the
//! name had no machine-readable home and `message` resolved `agents/*`
//! refs only.

use super::{assert_prefixed, noop_editor, with_fx};
use crate::cmd::{Outcome, dispatch, message, prompt};
use crate::prompt::inbox::{inbox_dir, try_acquire};
use crate::template::{GitRunner, RealGit};
use crate::workspace::{agent_ids, agent_name, fixture};
use std::path::{Path, PathBuf};

const PARENT: &str = "20260101T000000Z-aaaaaaaa";

/// Dispatch a worker child off `PARENT`, optionally named. The driver
/// target is `true`, so the front-door launch spawns a harmless no-op
/// instead of a real `litany advance` (the surface tests' convention).
fn dispatch_child(ws: &Path, name: Option<&str>) -> Result<Outcome, super::Error> {
    with_fx("true", b"", &noop_editor, |fx| {
        dispatch::run(
            dispatch::Args {
                role: "worker".into(),
                repo: ws.to_path_buf(),
                branch: PARENT.into(),
                goal: Some("do the thing".into()),
                from: None,
                name: name.map(str::to_owned),
                pin: vec![],
                cwd: None,
            },
            fx,
        )
    })
    .0
}

/// The one agent id that is not the parent — the child just forked.
fn child_of(ws: &Path) -> String {
    let mut ids = agent_ids(ws, &RealGit::new()).unwrap();
    ids.retain(|id| id != PARENT);
    assert_eq!(ids.len(), 1, "exactly one child was forked");
    ids.remove(0)
}

#[test]
fn a_dispatched_child_wears_its_name_and_message_resolves_it() {
    let (_h, ws) = fixture::workspace();
    fixture::spawn_root(&ws, PARENT);
    dispatch_child(&ws, Some("pale-otter")).unwrap();

    let child = child_of(&ws);
    assert_eq!(
        agent_name::read(&ws, &child, &RealGit::new()).as_deref(),
        Some("pale-otter"),
        "the name's one home is the child's own dispatch commit",
    );

    // Hold the child's lease so the post-deposit probe reads Busy and
    // launches nothing (§2.11).
    let _held = try_acquire(&inbox_dir(&ws, &child))
        .unwrap()
        .expect("free lease");
    let (r, ..) = with_fx("true", b"", &noop_editor, |fx| {
        message::run(
            message::Args {
                workspace: ws.clone(),
                agent: "pale-otter".into(),
                content: "steering".into(),
            },
            fx,
        )
    });
    assert!(matches!(r.unwrap(), Outcome::Quiet));
    // Beside the dispatch message the fork already deposited (§2.5),
    // one `user`-sender deposit — ours, addressed by name.
    let ours = std::fs::read_dir(inbox_dir(&ws, &child))
        .unwrap()
        .flatten()
        .filter(|e| e.file_name().to_string_lossy().starts_with("user-"))
        .count();
    assert_eq!(ours, 1, "the name addressed the child's own inbox");
}

#[test]
fn a_name_a_living_agent_wears_is_refused_at_both_creation_verbs_and_forks_nothing() {
    let (_h, ws) = fixture::workspace();
    fixture::spawn_root(&ws, PARENT);
    dispatch_child(&ws, Some("pale-otter")).unwrap();
    let before = agent_ids(&ws, &RealGit::new()).unwrap().len();

    let err = dispatch_child(&ws, Some("pale-otter")).unwrap_err();
    assert!(err.to_string().contains("already worn"), "{err}");
    assert_prefixed(err, "dispatch worker");

    let (r, ..) = with_fx("true", b"", &noop_editor, |fx| {
        prompt::run(
            prompt::Args {
                repo: ws.clone(),
                message: "hi".into(),
                from: None,
                config: None,
                name: Some("pale-otter".into()),
                pin: vec![],
                cwd: None,
            },
            fx,
        )
    });
    let err = r.unwrap_err();
    assert!(err.to_string().contains("already worn"), "{err}");
    assert_prefixed(err, "prompt");

    assert_eq!(
        agent_ids(&ws, &RealGit::new()).unwrap().len(),
        before,
        "a refused name leaves no branch behind",
    );
}

#[test]
fn an_ambiguous_name_is_refused_with_its_candidates_and_deposits_nothing() {
    let (_h, ws) = fixture::workspace();
    let git = RealGit::new();
    // Two living agents wearing one name — reachable by a fork-back-in
    // off a named commit (§2.3), which creation-time uniqueness cannot
    // see. Resolution refuses rather than guesses.
    for id in [PARENT, "20260102T000000Z-bbbbbbbb"] {
        let wt = fixture::spawn_root(&ws, id);
        agent_name::settle(&wt, Some("pale-otter"), &git).unwrap();
        git.run(&wt, &["commit", "-m", "settle name"]).unwrap();
    }
    let (r, ..) = with_fx("true", b"", &noop_editor, |fx| {
        message::run(
            message::Args {
                workspace: ws.clone(),
                agent: "pale-otter".into(),
                content: "steering".into(),
            },
            fx,
        )
    });
    let err = r.unwrap_err();
    let msg = err.to_string();
    assert!(msg.contains("ambiguous"), "{msg}");
    assert!(msg.contains(PARENT), "{msg}");
    assert!(msg.contains("20260102T000000Z-bbbbbbbb"), "{msg}");
    assert_prefixed(err, "message");
    assert!(
        !inbox_dir(&ws, "pale-otter").exists(),
        "an ambiguous needle deposits nothing"
    );
}

#[test]
fn an_unknown_name_gets_the_shared_existence_decline_and_it_mentions_names() {
    let (_h, ws) = fixture::workspace();
    let (r, ..) = with_fx("true", b"", &noop_editor, |fx| {
        message::run(
            message::Args {
                workspace: PathBuf::from(&ws),
                agent: "grey-heron".into(),
                content: "hi".into(),
            },
            fx,
        )
    });
    let err = r.unwrap_err();
    let msg = err.to_string();
    assert!(msg.contains("no agent \"grey-heron\""), "{msg}");
    assert!(msg.contains("by id or unique name"), "{msg}");
}

#[test]
fn a_child_of_a_named_parent_wears_its_own_mint_not_its_parents_namesake() {
    // The regression the always-written file exists to make impossible
    // (§2.3): a child forks off its parent's tip and so inherits the
    // parent's `name` blob. Its own dispatch commit settles the fact —
    // since yog bl-aca4 an omitted name is *minted*, so the child wears
    // its own one-word name, never the inherited one, and uniqueness
    // holds.
    let (_h, ws) = fixture::workspace();
    fixture::spawn_root(&ws, PARENT);
    dispatch_child(&ws, Some("pale-otter")).unwrap();
    let named = child_of(&ws);

    // A grandchild off the *named* child, dispatched with no name.
    with_fx("true", b"", &noop_editor, |fx| {
        dispatch::run(
            dispatch::Args {
                role: "worker".into(),
                repo: ws.clone(),
                branch: named.clone(),
                goal: Some("go".into()),
                from: None,
                name: None,
                pin: vec![],
                cwd: None,
            },
            fx,
        )
    })
    .0
    .unwrap();

    let git = RealGit::new();
    let grandchild = agent_ids(&ws, &git)
        .unwrap()
        .into_iter()
        .find(|id| id.starts_with(&format!("{named}-")))
        .expect("the grandchild forked");
    let minted = agent_name::read(&ws, &grandchild, &git)
        .expect("an omitted name is minted — no fork ends nameless (yog bl-aca4)");
    assert_ne!(
        minted, "pale-otter",
        "a child does not wear the name it inherited",
    );
    assert!(
        crate::workspace::agent_name::mint::is_minted_shape(&minted),
        "a minted name is two PascalCase words (bl-79a2), got {minted:?}",
    );
    assert_eq!(
        agent_name::read(&ws, &named, &git).as_deref(),
        Some("pale-otter"),
        "and its parent keeps its own",
    );
}
