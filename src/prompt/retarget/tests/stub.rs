//! The git-op error arms of the retarget landing, routed through a
//! scripted stub keyed on the git subcommand, so every `map_err` in
//! [`super::super`] and [`base`] is reachable without constructing the
//! corresponding real-git failure. The behavioral arms live in [`super`].

use super::*;
use std::cell::RefCell;

/// Scripted git: `fail_run` fails the first `run` whose argv (joined)
/// contains the pattern; `fail_capture` likewise for captures;
/// `rebase_fails` fails that many non-`--abort` rebase invocations, each
/// stop then consulting `ls_files`.
struct Script {
    fail_run: Option<&'static str>,
    fail_capture: Option<&'static str>,
    rebase_fails: RefCell<u32>,
    ls_files: &'static str,
    /// `log` answer — the founding sha, and the dispatch subject.
    log: &'static str,
    /// `rev-parse` answer — the governing config and the target alike, so
    /// the default script is deliberately *not* a no-op (below).
    rev_parse: &'static str,
    /// `show`/`cat-file` answer: the target's `providers.yaml` and soul.
    providers: &'static str,
}

impl Default for Script {
    fn default() -> Self {
        Self {
            fail_run: None,
            fail_capture: None,
            rebase_fails: RefCell::new(0),
            ls_files: "",
            log: "dsha",
            rev_parse: "gsha",
            providers: "roles:\n  worker:\n    provider: p\n    model: m\n",
        }
    }
}

impl Script {
    /// Land against the scripted git. The mark is read through the same
    /// stub, so `rev-parse` answers it too — `target` is what the caller
    /// passes and `gsha` is what governs, which differ, so the ordinary
    /// script is a real landing rather than a no-op.
    fn land(&self) -> Result<Option<Outcome>, Error> {
        super::super::land(Path::new("/ws"), "a", Path::new("/ws/agents/a"), self)
    }
}

impl crate::template::GitRunner for Script {
    fn run(&self, _d: &Path, args: &[&str]) -> std::io::Result<()> {
        let joined = args.join(" ");
        if let Some(pat) = self.fail_run
            && joined.contains(pat)
        {
            return Err(std::io::Error::other("stub fail"));
        }
        // The scratch worktree is a real directory in production; the
        // stub makes one so the trim has a tree to write into.
        if joined.starts_with("worktree add") {
            std::fs::create_dir_all(args[args.len() - 2]).unwrap();
        }
        if joined.contains("rebase") && !joined.contains("--abort") {
            let mut left = self.rebase_fails.borrow_mut();
            if *left > 0 {
                *left -= 1;
                return Err(std::io::Error::other("rebase stop"));
            }
        }
        Ok(())
    }
    fn run_capture(&self, _d: &Path, args: &[&str]) -> std::io::Result<String> {
        let joined = args.join(" ");
        if let Some(pat) = self.fail_capture
            && joined.contains(pat)
        {
            return Err(std::io::Error::other("stub fail"));
        }
        Ok(match args.first().copied() {
            // The retarget mark reads as a target that is not `gsha`.
            Some("rev-parse") if joined.contains("retarget") => "tsha".into(),
            Some("rev-parse") => self.rev_parse.into(),
            Some("log") => self.log.into(),
            Some("merge-base") => self.rev_parse.into(),
            // `config_names` asks for short names, `config_lineage` for
            // full ones, and the follow-the-tip derivation (§2.2,
            // bl-403b) for tips — three readings of one registry (§2.3).
            // The tip answers `gsha`, so the followed answer stays the
            // governing sha and the default script's mark (`tsha`) is a
            // real landing rather than a no-op.
            Some("for-each-ref") if joined.contains(":short") => "config/default\n".into(),
            Some("for-each-ref") if joined.contains("%(objectname)") => "gsha\n".into(),
            Some("for-each-ref") => "refs/heads/config/default\n".into(),
            Some("show") => self.providers.into(),
            Some("write-tree") => "wsha".into(),
            Some("commit-tree") => "bsha".into(),
            Some("ls-files") => self.ls_files.into(),
            // The replay's `rev-list --count` — one commit to replay.
            _ => "1".into(),
        })
    }
}

fn assert_op(err: Error, want: &str) {
    match err {
        Error::Git { op, .. } => assert_eq!(op, want),
        other => panic!("{other:?}"),
    }
}

#[test]
fn a_clean_scripted_landing_lands() {
    assert_eq!(Script::default().land().unwrap(), Some(Outcome::Landed));
}

#[test]
fn an_unreadable_mark_lands_nothing() {
    let s = Script {
        fail_capture: Some("retarget"),
        ..Script::default()
    };
    assert_eq!(s.land().unwrap(), None);
}

#[test]
fn a_governing_config_derivation_failure_surfaces() {
    let s = Script {
        fail_capture: Some("merge-base"),
        ..Script::default()
    };
    assert_op(s.land().unwrap_err(), "retarget followed config");
}

#[test]
fn a_missing_dispatch_commit_is_declined() {
    let s = Script {
        log: "",
        ..Script::default()
    };
    assert_op(s.land().unwrap_err(), "retarget dispatch commit");
}

#[test]
fn a_clear_failure_surfaces_after_the_landing() {
    let s = Script {
        fail_run: Some("update-ref -d"),
        ..Script::default()
    };
    assert_op(s.land().unwrap_err(), "retarget clear mark");
}

#[test]
fn the_role_derivation_and_grant_read_failures_surface() {
    let s = Script {
        fail_capture: Some("--format=%s -E"),
        ..Script::default()
    };
    assert!(s.land().is_err());
    let s = Script {
        fail_capture: Some("providers.yaml"),
        ..Script::default()
    };
    assert!(matches!(s.land().unwrap_err(), Error::ControlRead { .. }));
}

#[test]
fn a_soul_the_target_config_does_not_carry_surfaces() {
    // The soul is re-pinned from the target's `souls/<role>.md` (§2.3);
    // a target that carries none for the branch's role is a control read
    // that fails, named by its `<commit>:<path>` address.
    let s = Script {
        fail_capture: Some("souls/"),
        ..Script::default()
    };
    assert!(matches!(s.land().unwrap_err(), Error::ControlRead { .. }));
}

#[test]
fn a_malformed_target_providers_yaml_surfaces() {
    let s = Script {
        providers: "roles: [not, a, map]\n",
        ..Script::default()
    };
    assert!(matches!(s.land().unwrap_err(), Error::Config(_)));
}

#[test]
fn scratch_worktree_and_mint_failures_surface() {
    for (pat, op) in [
        ("worktree add", "retarget scratch worktree"),
        ("add -A", "retarget add"),
    ] {
        let s = Script {
            fail_run: Some(pat),
            ..Script::default()
        };
        assert_op(s.land().unwrap_err(), op);
    }
    for (pat, op) in [
        ("write-tree", "retarget write-tree"),
        ("commit-tree", "retarget commit-tree"),
    ] {
        let s = Script {
            fail_capture: Some(pat),
            ..Script::default()
        };
        assert_op(s.land().unwrap_err(), op);
    }
}

#[test]
fn a_dispatch_subject_read_failure_surfaces() {
    let s = Script {
        fail_capture: Some("--format=%s dsha"),
        ..Script::default()
    };
    assert_op(s.land().unwrap_err(), "retarget dispatch subject");
}

// Stages 2+3 on one path: both sides carry content, so git wrote markers.
const BOTH_SIDES: &str = "100644 bbb 2\tsummary/001.md\n100644 ccc 3\tsummary/001.md\n";

#[test]
fn a_content_conflict_declines_with_the_paths_and_marks_the_branch() {
    let s = Script {
        rebase_fails: RefCell::new(1),
        ls_files: BOTH_SIDES,
        ..Script::default()
    };
    assert_eq!(
        s.land().unwrap(),
        Some(Outcome::Conflicted(vec!["summary/001.md".to_string()])),
    );
}

/// [`preflight`] against the scripted git, for the two arms a real
/// workspace cannot reach: a lineage that exists but will not resolve to a
/// commit, and the resolve that succeeds.
mod preflighting {
    use super::*;
    use crate::prompt::retarget::preflight;

    /// A directory that passes the layout guard (`repo.git/` present) and
    /// nothing more — every other question is the script's to answer.
    fn shell() -> tempfile::TempDir {
        let d = tempfile::TempDir::new().unwrap();
        std::fs::create_dir_all(d.path().join("repo.git")).unwrap();
        d
    }

    #[test]
    fn a_lineage_that_will_not_resolve_to_a_commit_surfaces() {
        let d = shell();
        let s = Script {
            fail_capture: Some("^{commit}"),
            ..Script::default()
        };
        assert_op(
            preflight(d.path(), "a", "default", &s).unwrap_err(),
            "retarget resolve target",
        );
    }

    #[test]
    fn a_target_that_already_governs_resolves_to_none() {
        // The script answers the same sha to `rev-parse` and to the
        // ancestry derivation, which is exactly "already governing".
        let d = shell();
        assert_eq!(
            preflight(d.path(), "a", "default", &Script::default()).unwrap(),
            None,
        );
    }
}
