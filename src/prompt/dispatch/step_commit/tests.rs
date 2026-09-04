//! The system slot's composition (ARCH §2.3 *Goal and soul are pinned
//! files*, §2.8): goal, identity, soul — one string, three tree facts.

use super::compose_system;

#[test]
fn an_unnamed_agent_states_no_identity() {
    // Byte-identical to the pre-name slot: absence is the general path
    // with empty inputs, so no blank line and no empty sentence is left
    // behind for the model to read something into (§2.8).
    assert_eq!(
        compose_system("hello", None, "system body"),
        "<goal>\nhello\n</goal>\n\nsystem body"
    );
}

#[test]
fn a_named_agent_is_told_its_name_between_the_goal_and_the_soul() {
    let slot = compose_system("hello", Some("pale-otter"), "system body");
    assert_eq!(
        slot,
        "<goal>\nhello\n</goal>\n\nYour name is pale-otter.\n\nsystem body"
    );
    // The goal still leads, which is the whole of §2.8's pinning claim.
    assert!(slot.starts_with("<goal>"), "{slot}");
    // One sentence, no instruction attached (§2.8).
    assert_eq!(slot.matches("pale-otter").count(), 1, "{slot}");
}

/// The trim's second part: the config lineage's **workspace skill**
/// bodies leave the forked tree, an agent's elected ones stay
/// (`docs/DESIGN_LEARNING_LOOP.md` §3, ARCH §2.7).
mod skill_bodies {
    use crate::prompt::Error;
    use crate::prompt::dispatch::{Grant, trim_to_context};
    use crate::template::{GitRunner, RealGit};
    use crate::workspace::{config_ref, fixture, repo_git};
    use std::path::{Path, PathBuf};

    fn manifest(name: &str) -> String {
        format!("---\nname: {name}\ndescription: d\n---\nbody\n")
    }

    /// A workspace whose lineage carries `skills/notes/`, and a root
    /// agent forked off that tip — so the fork inherits the body and the
    /// trim is what must take it away.
    fn forked_off_a_lineage_with_a_skill() -> (tempfile::TempDir, PathBuf, PathBuf) {
        let (holder, ws) = fixture::workspace();
        fixture::amend_config(
            &ws,
            &[("skills/notes/SKILL.md", manifest("notes").as_str())],
        );
        let wt = fixture::spawn_root(&ws, "a1");
        (holder, ws, wt)
    }

    fn config_tip(ws: &Path) -> String {
        RealGit::new()
            .run_capture(&repo_git(ws), &["rev-parse", &config_ref("default")])
            .unwrap()
    }

    fn trim(wt: &Path, commit: &str, git: &dyn GitRunner) -> Result<(), Error> {
        let grant = Grant {
            role: "worker",
            tools: &[],
            config_commit: commit,
        };
        trim_to_context(wt, "20260101-t1", &grant, None, git)
    }

    #[test]
    fn the_lineages_body_leaves_and_an_elected_one_stays() {
        let (_h, ws, wt) = forked_off_a_lineage_with_a_skill();
        let git = RealGit::new();
        // An elected body the config commit does not carry — what
        // `load_skill` leaves behind, and §2.7's compactor input.
        std::fs::create_dir_all(wt.join("skills/elected")).unwrap();
        std::fs::write(wt.join("skills/elected/SKILL.md"), manifest("elected")).unwrap();
        git.run(&wt, &["add", "skills/elected"]).unwrap();
        git.run(&wt, &["commit", "-m", "elect"]).unwrap();

        trim(&wt, &config_tip(&ws), &git).unwrap();

        assert!(
            !wt.join("skills/notes").exists(),
            "the lineage's body stays behind"
        );
        assert!(
            wt.join("skills/elected/SKILL.md").exists(),
            "an elected body is context"
        );
    }

    #[test]
    fn a_tree_that_carries_no_skills_costs_no_git_command() {
        // Every fork off a lineage that has authored no workspace skill:
        // the enumeration is empty and nothing is staged.
        let (_h, ws) = fixture::workspace();
        let wt = fixture::spawn_root(&ws, "a1");
        assert!(!wt.join("skills").exists());
        trim(&wt, &config_tip(&ws), &RealGit::new()).unwrap();
    }

    #[test]
    fn a_refused_removal_surfaces_as_a_git_error() {
        struct RefusesSkillRm(RealGit);
        impl GitRunner for RefusesSkillRm {
            fn run(&self, dest: &Path, args: &[&str]) -> std::io::Result<()> {
                if args.iter().any(|a| a.starts_with("skills/")) {
                    return Err(std::io::Error::other("rm refused"));
                }
                self.0.run(dest, args)
            }
            fn run_capture(&self, dest: &Path, args: &[&str]) -> std::io::Result<String> {
                self.0.run_capture(dest, args)
            }
        }
        let (_h, ws, wt) = forked_off_a_lineage_with_a_skill();
        let err = trim(&wt, &config_tip(&ws), &RefusesSkillRm(RealGit::new())).unwrap_err();
        assert!(
            matches!(&err, Error::Git { op, .. } if *op == "rm the config's skill bodies"),
            "{err}"
        );
    }

    #[test]
    fn an_unreadable_skills_path_surfaces_rather_than_reading_as_empty() {
        // `skills` as a *file* is not "no skills": a read failure that is
        // not NotFound must not be answered as an empty enumeration.
        let (_h, ws) = fixture::workspace();
        let wt = fixture::spawn_root(&ws, "a1");
        std::fs::write(wt.join("skills"), b"not a directory").unwrap();
        let err = trim(&wt, &config_tip(&ws), &RealGit::new()).unwrap_err();
        assert!(matches!(err, Error::Io(_)), "{err}");
    }
}

/// The trim's fifth part: a **reviewer's** fresh read of the config
/// commit's workspace skills (`docs/DESIGN_LEARNING_LOOP.md` §2). Its
/// other proposable class, the facts document, is the trim's fourth
/// part above — every fork reads it in, so the reviewer needs no second
/// checkout of it. The happy paths run against the real checkpoint in
/// `child_result::tests::flush_reviewer`; what is only reachable here is
/// the decline.
mod reviewer_read {
    use crate::prompt::Error;
    use crate::prompt::dispatch::{Grant, trim_to_context};
    use crate::template::{GitRunner, RealGit};
    use crate::workspace::{config_ref, fixture, repo_git};
    use std::path::Path;

    #[test]
    fn a_refused_checkout_surfaces_as_a_git_error() {
        struct RefusesCheckout(RealGit);
        impl GitRunner for RefusesCheckout {
            fn run(&self, dest: &Path, args: &[&str]) -> std::io::Result<()> {
                if args.first() == Some(&"checkout") && args.contains(&"skills") {
                    return Err(std::io::Error::other("checkout refused"));
                }
                self.0.run(dest, args)
            }
            fn run_capture(&self, dest: &Path, args: &[&str]) -> std::io::Result<String> {
                self.0.run_capture(dest, args)
            }
        }
        let (_h, ws) = fixture::workspace();
        fixture::amend_config(
            &ws,
            &[(
                "skills/notes/SKILL.md",
                "---\nname: notes\ndescription: d\n---\nbody\n",
            )],
        );
        let wt = fixture::spawn_root(&ws, "a1");
        let tip = RealGit::new()
            .run_capture(&repo_git(&ws), &["rev-parse", &config_ref("default")])
            .unwrap();
        let grant = Grant {
            role: "reviewer",
            tools: &[],
            config_commit: &tip,
        };
        let err = trim_to_context(
            &wt,
            "20260101-t1",
            &grant,
            None,
            &RefusesCheckout(RealGit::new()),
        )
        .unwrap_err();
        assert!(
            matches!(&err, Error::Git { op, .. } if *op == "checkout the reviewer's read"),
            "{err}"
        );
    }

    #[test]
    fn a_refused_read_mark_surfaces_as_a_git_error() {
        // The mark is the reviewer's landing's only record of which
        // config commit it read (`docs/DESIGN_LEARNING_LOOP.md` §3 step
        // 4), so a dispatch that cannot write it must fail loudly rather
        // than fork a reviewer whose proposal can never be parented.
        struct RefusesMark(RealGit);
        impl GitRunner for RefusesMark {
            fn run(&self, dest: &Path, args: &[&str]) -> std::io::Result<()> {
                if args.first() == Some(&"update-ref") {
                    return Err(std::io::Error::other("update-ref refused"));
                }
                self.0.run(dest, args)
            }
            fn run_capture(&self, dest: &Path, args: &[&str]) -> std::io::Result<String> {
                self.0.run_capture(dest, args)
            }
        }
        let (_h, ws) = fixture::workspace();
        let wt = fixture::spawn_root(&ws, "a2");
        let tip = RealGit::new()
            .run_capture(&repo_git(&ws), &["rev-parse", &config_ref("default")])
            .unwrap();
        let grant = Grant {
            role: "reviewer",
            tools: &[],
            config_commit: &tip,
        };
        let err = trim_to_context(
            &wt,
            "20260101-t2",
            &grant,
            None,
            &RefusesMark(RealGit::new()),
        )
        .unwrap_err();
        assert!(
            matches!(&err, Error::Git { op, .. } if *op == "mark the reviewer's read"),
            "{err}"
        );
    }
}
