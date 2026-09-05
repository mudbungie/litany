//! **What the reviewer wrote, whether it may be proposed, and the patch
//! that carries it** — §3 steps 2, 3 and 6 of
//! `docs/DESIGN_LEARNING_LOOP.md`, split from [`super`] so the landing
//! there reads as the act it performs.
//!
//! The two git reads here take the **same range and the same subject**
//! by construction: the names are read with the transcript excluded, and
//! the patch is written with the two admitted classes included. Since a
//! proposal is minted only when every name is admitted, the two agree —
//! and neither can ever carry a transcript entry into a config commit.

use super::{MESSAGES_DIR, SKILLS_DIR, SUMMARY_DIR};
use crate::facts::FILE as FACTS_FILE;
use crate::prompt::Error;
use crate::prompt::dispatch::step_commit::DESCRIPTIONS_DIR;
use crate::template::GitRunner;
use std::collections::BTreeSet;
use std::path::Path;

/// The paths the reviewer's own commits changed: its founding commit's
/// tree against its terminal ref's, with everything **the harness
/// writes on the branch** excluded — `messages/**` by every branch's
/// executor (ARCH §2.3), `summary/**` by a compaction landing, and
/// `descriptions/**` by the descriptor cut, which is re-derived from the
/// governing config commit at every step boundary under follow-the-tip
/// (§3.3, bl-37cd). None of the three is a reviewer's edit, so none is a
/// proposal's business — and the third would otherwise refuse the whole
/// proposal as *Outside* whenever the config tip moved mid-review, which
/// is precisely when a review is most worth having.
pub(super) fn changed_paths(
    worktree: &Path,
    founding: &str,
    terminal: &str,
    git: &dyn GitRunner,
) -> Result<Vec<String>, Error> {
    let (messages, summary, descriptions) = (
        format!(":(exclude){MESSAGES_DIR}"),
        format!(":(exclude){SUMMARY_DIR}"),
        format!(":(exclude){DESCRIPTIONS_DIR}"),
    );
    let out = git
        .run_capture(
            worktree,
            &[
                "diff",
                "--name-only",
                founding,
                terminal,
                "--",
                &messages,
                &summary,
                &descriptions,
            ],
        )
        .map_err(|source| Error::Git {
            op: "proposal diff",
            source,
        })?;
    Ok(out
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(str::to_string)
        .collect())
}

/// Is `path` one of the two proposable classes (§3 step 3, §4)?
///
/// A `skills/` path is admitted when it names a **body inside a skill
/// directory** whose name the install pool does not hold. That one test
/// answers both halves of the rule at once: a workspace skill's name can
/// never be a pool name (the authoring pass refuses the collision, ARCH
/// §3.3), so "a workspace skill, or a new name the pool does not hold"
/// *is* "a name the pool does not hold". `skills/<file>` — a loose file
/// where a directory belongs — is not the class and refuses.
pub(super) fn admitted(path: &str, pool: &BTreeSet<String>) -> bool {
    if path == FACTS_FILE {
        return true;
    }
    let mut parts = path.split('/');
    if parts.next() != Some(SKILLS_DIR) {
        return false;
    }
    let Some(name) = parts.next() else {
        return false;
    };
    parts.next().is_some() && !pool.contains(name)
}

/// Write the reviewer's edits as a patch file at `out`, restricted to
/// the two proposable classes — the edit step the config-authoring
/// routine applies inside its transient checkout (§3 step 6). The
/// pathspec is positive where [`changed_paths`]'s is negative, and that
/// is the same agreement stated the other way round: what a proposal
/// commits is exactly what [`admitted`] passed.
pub(super) fn write_patch(
    worktree: &Path,
    founding: &str,
    terminal: &str,
    out: &str,
    git: &dyn GitRunner,
) -> Result<(), Error> {
    let output = format!("--output={out}");
    git.run(
        worktree,
        &[
            "diff", founding, terminal, &output, "--", SKILLS_DIR, FACTS_FILE,
        ],
    )
    .map_err(|source| Error::Git {
        op: "proposal patch",
        source,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io;

    /// A `GitRunner` answering one scripted `diff --name-only` and
    /// recording the argv it was asked for.
    struct DiffGit(&'static str, std::cell::RefCell<Vec<String>>);
    impl GitRunner for DiffGit {
        fn run(&self, _d: &Path, _a: &[&str]) -> io::Result<()> {
            unreachable!("the filter only captures")
        }
        fn run_capture(&self, _d: &Path, args: &[&str]) -> io::Result<String> {
            *self.1.borrow_mut() = args.iter().map(|s| (*s).to_string()).collect();
            if self.0.is_empty() {
                return Err(io::Error::other("diff boom"));
            }
            Ok(self.0.to_string())
        }
    }

    fn pool(names: &[&str]) -> BTreeSet<String> {
        names.iter().map(|n| (*n).to_string()).collect()
    }

    #[test]
    fn the_two_classes_are_admitted_and_nothing_else() {
        let pool = pool(&["bash"]);
        assert!(admitted("skills/notes/SKILL.md", &pool));
        assert!(admitted("skills/notes/ref/table.md", &pool));
        assert!(admitted(FACTS_FILE, &pool));
        // The archive container is a move a proposal may carry (§5), and
        // no pool skill may be named for it (ARCH §3.3).
        assert!(admitted("skills/archived/notes/SKILL.md", &pool));
        // A name the install pool holds: a loaded pool copy, not the
        // workspace's to edit.
        assert!(!admitted("skills/bash/SKILL.md", &pool));
        // Not the class: a work product, a control file, a loose file
        // where a skill directory belongs.
        assert!(!admitted("out.txt", &pool));
        assert!(!admitted("soul.md", &pool));
        assert!(!admitted("skills/loose.txt", &pool));
        assert!(!admitted("skills", &pool));
    }

    #[test]
    fn the_transcript_is_excluded_by_pathspec_and_blank_lines_drop() {
        let git = DiffGit(
            "skills/notes/SKILL.md

",
            std::cell::RefCell::new(Vec::new()),
        );
        let changed = changed_paths(Path::new("/wt"), "f", "t", &git).unwrap();
        assert_eq!(changed, vec!["skills/notes/SKILL.md".to_string()]);
        let args = git.1.borrow();
        assert!(args.iter().any(|a| a == ":(exclude)messages"), "{args:?}");
        assert!(args.iter().any(|a| a == ":(exclude)summary"), "{args:?}");
    }

    /// A `GitRunner` whose `run` fails — the patch write's only failure
    /// mode, and the one call it makes.
    struct FailingRun;
    impl GitRunner for FailingRun {
        fn run(&self, _d: &Path, _a: &[&str]) -> io::Result<()> {
            Err(io::Error::other("diff --output boom"))
        }
        fn run_capture(&self, _d: &Path, _a: &[&str]) -> io::Result<String> {
            unreachable!("the patch write never captures")
        }
    }

    #[test]
    fn a_failing_patch_write_surfaces_as_a_git_error() {
        let err = write_patch(Path::new("/wt"), "f", "t", "/tmp/p", &FailingRun).unwrap_err();
        assert!(
            matches!(
                err,
                Error::Git {
                    op: "proposal patch",
                    ..
                }
            ),
            "{err:?}"
        );
    }

    #[test]
    fn a_failing_diff_surfaces_as_a_git_error() {
        let git = DiffGit("", std::cell::RefCell::new(Vec::new()));
        let err = changed_paths(Path::new("/wt"), "f", "t", &git).unwrap_err();
        assert!(
            matches!(
                err,
                Error::Git {
                    op: "proposal diff",
                    ..
                }
            ),
            "{err:?}"
        );
    }
}
