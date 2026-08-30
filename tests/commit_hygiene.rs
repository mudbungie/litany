//! Commit-identity guard for `refs/heads/main` — armed per machine, off by
//! default.
//!
//! `main`'s history was normalized on 2026-07-26: every author and committer
//! became `mudbungie <mudbungie@gmail.com>`, and every `Co-Authored-By`
//! trailer was stripped. This test keeps it that way on the machine that did
//! the normalizing, without imposing anything on public CI or on anybody
//! else's clone. The invariant it holds is **one set of identities**, in all
//! three slots — author, committer, and any co-author trailer — rather than
//! the trailer's absence; see [`coauthor_allowed`] for why the difference
//! matters to a release commit.
//!
//! The switch is a file OUTSIDE the repo:
//! `$XDG_CONFIG_HOME/litany/enforce-commit-identity` (default
//! `~/.config/litany/enforce-commit-identity`). Absent — the default
//! everywhere but here — the test passes without asserting anything. Present,
//! it walks the whole of `refs/heads/main` and asserts the invariants below.
//! The policy lives in the marker, not in this file: `rm` it and the guard is
//! off, with no code edit and no flag.
//!
//! `LITANY_HOME` is deliberately NOT consulted, though it collapses the
//! harness roots everywhere else (ARCH §2.2, `src/harness_root.rs`): those
//! roots are per-installation state that tests and wrappers relocate freely
//! (`tests/install.rs` aims `LITANY_HOME` at a tempdir), so a guard that
//! followed them could be disarmed by an inherited env var without anyone
//! noticing. This marker is per-machine operator policy, so it has exactly
//! one path.
//!
//! Scope: the guard reads a ref, never a working tree, so it is indifferent
//! to which worktree runs it — every worktree of this repo shares
//! `refs/heads/main`. It asserts over the FULL history because the history is
//! clean as of the rewrite; there is no baseline cursor to keep in sync.

use std::path::{Path, PathBuf};
use std::process::Command;

/// The one human identity `main` is allowed to carry.
///
/// The **email** is the identity; the name beside it is a display string
/// nobody controls end to end. GitHub's merge button authors with the
/// account's *profile* name against the same verified address — `Mud Bungie
/// <mudbungie@gmail.com>` on the PR #2 merge (`ceee487`) — which is the same
/// person by every fact that matters and which no local git config can
/// change. Matching the pair made the guard fail on a merge the operator
/// made through the web UI, so it matches the address and leaves the display
/// name alone. Nothing is loosened that the guard was protecting: a foreign
/// author has a foreign address, and the FORBIDDEN sweep below still reads
/// names.
const OWNER_EMAIL: &str = "mudbungie@gmail.com";

/// GitHub's Actions bot stays allowed BY DESIGN: release-plz authors the
/// version-bump/changelog commit as it and pushes that to `main`, so
/// rejecting the bot would turn every release into a red test on this
/// machine. Matched on the name alone — GitHub mints the address as
/// `<numeric-id>+github-actions[bot]@users.noreply.github.com`, and pinning
/// that id would make the guard brittle for no gain.
const CI_BOT_NAME: &str = "github-actions[bot]";

/// The committer GitHub stamps on anything done through the web UI — the
/// merge button included, which is how PR #2 landed (`ceee487`). It is the
/// same class of allowance as [`CI_BOT_NAME`]: a GitHub-minted machine
/// identity on an action only a repo admin can take, appearing beside the
/// operator's own address in the author slot. Refusing it would mean either
/// never merging a PR from the web or rewriting `main` after every merge.
const WEB_UI_COMMITTER_EMAIL: &str = "noreply@github.com";

/// Substrings that must appear in no identity and no message, lowercase for
/// case-insensitive matching: the throwaway test identity the rewrite erased
/// (it came from a repo-local `user.email`, since removed), and the
/// operator's personal address, which belongs in a credential store rather
/// than in a public history.
const FORBIDDEN: [&str; 2] = ["t@t.local", "orionriver"];

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

/// The arming marker. An empty `XDG_CONFIG_HOME` counts as unset, matching
/// the XDG spec and `src/harness_root.rs`.
fn marker() -> PathBuf {
    let config_home = std::env::var_os("XDG_CONFIG_HOME")
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            PathBuf::from(std::env::var_os("HOME").unwrap_or_default()).join(".config")
        });
    config_home.join("litany").join("enforce-commit-identity")
}

/// Every commit on `main` as NUL-separated records of unit-separator-
/// delimited fields: sha, author name/email, committer name/email, raw
/// message. `None` when `git` or the ref is unavailable — a checkout that
/// cannot answer is not a checkout this guard may judge.
///
/// The git env is scrubbed for the same reason `tests/hooks.rs` scrubs it: a
/// run under the pre-commit gate inherits `GIT_DIR`/`GIT_INDEX_FILE` from the
/// hook that spawned it, and the guard must read this repo's own ref rather
/// than whatever the ambient environment points at.
fn main_history(root: &Path) -> Option<String> {
    let out = Command::new("git")
        .current_dir(root)
        .env_remove("GIT_DIR")
        .env_remove("GIT_WORK_TREE")
        .env_remove("GIT_INDEX_FILE")
        .args([
            "log",
            "-z",
            "--format=%H%x1f%an%x1f%ae%x1f%cn%x1f%ce%x1f%B",
            "refs/heads/main",
        ])
        .output()
        .ok()?;
    out.status
        .success()
        .then(|| String::from_utf8_lossy(&out.stdout).into_owned())
}

fn allowed(name: &str, email: &str) -> bool {
    email == OWNER_EMAIL || email == WEB_UI_COMMITTER_EMAIL || name == CI_BOT_NAME
}

/// Does a `Co-Authored-By` trailer line name one of the allowed identities?
///
/// The trailer is judged by the SAME three identities as the author and
/// committer slots — a **foreign identity** is what this guard is for, and a
/// trailer is a third slot for one, not a different offence. A blanket "no
/// trailer at all" contradicted the [`CI_BOT_NAME`] allowance outright:
/// GitHub's merge button stamps `Co-authored-by: github-actions[bot]` onto
/// every squashed release PR, so the identity release-plz authors *as* was
/// allowed in the author slot and refused in the trailer of the very same
/// commit (`v0.0.11`, which then reddened every close on the armed machine
/// until this ball).
///
/// Matched as a substring rather than by parsing `Name <address>` out: the
/// trailer's grammar is git's, not this guard's, and a parse gives the guard
/// a failure mode of its own (an unreadable trailer) where a substring gives
/// it none. Nothing is loosened — a foreign co-author carries neither the
/// owner's address nor a GitHub-minted machine name, and the `FORBIDDEN`
/// sweep below reads the same line again for the erased identities.
fn coauthor_allowed(line: &str) -> bool {
    line.contains(OWNER_EMAIL)
        || line.contains(WEB_UI_COMMITTER_EMAIL)
        || line.contains(CI_BOT_NAME)
}

#[test]
fn main_carries_one_identity_and_no_foreign_coauthors() {
    let marker = marker();
    if !marker.exists() {
        eprintln!(
            "commit-identity guard disarmed: {} does not exist",
            marker.display()
        );
        return;
    }
    let root = repo_root();
    let Some(log) = main_history(&root) else {
        eprintln!(
            "commit-identity guard skipped: no readable refs/heads/main under {}",
            root.display()
        );
        return;
    };

    let mut commits = 0usize;
    for record in log.split('\0').filter(|r| !r.is_empty()) {
        let mut fields = record.split('\u{1f}');
        let (Some(sha), Some(an), Some(ae), Some(cn), Some(ce), Some(message)) = (
            fields.next(),
            fields.next(),
            fields.next(),
            fields.next(),
            fields.next(),
            fields.next(),
        ) else {
            panic!("unparsable git log record: {record:?}");
        };
        commits += 1;

        assert!(
            allowed(an, ae),
            "{sha}: author `{an} <{ae}>` is not an allowed identity (<{OWNER_EMAIL}>, <{WEB_UI_COMMITTER_EMAIL}>, {CI_BOT_NAME})",
        );
        assert!(
            allowed(cn, ce),
            "{sha}: committer `{cn} <{ce}>` is not an allowed identity (<{OWNER_EMAIL}>, <{WEB_UI_COMMITTER_EMAIL}>, {CI_BOT_NAME})",
        );
        for line in message
            .lines()
            .filter(|l| l.to_lowercase().starts_with("co-authored-by:"))
        {
            assert!(
                coauthor_allowed(line),
                "{sha}: co-author trailer `{}` is not an allowed identity (<{OWNER_EMAIL}>, <{WEB_UI_COMMITTER_EMAIL}>, {CI_BOT_NAME})",
                line.trim(),
            );
        }
        let haystack = format!("{an} {ae} {cn} {ce} {message}").to_lowercase();
        for needle in FORBIDDEN {
            assert!(
                !haystack.contains(needle),
                "{sha}: history mentions {needle}"
            );
        }
    }
    assert!(
        commits > 0,
        "refs/heads/main reported no commits — the guard asserted nothing",
    );
}
