//! Agent role derivation from the dispatch commit subject (ARCH §2.5, §6).
//!
//! **Single authoritative home** (`docs/PRINCIPLES.md` Single source of
//! truth): a child agent's role lives in its **dispatch commit subject**,
//! `dispatch: <role> [<agent-id>]`, written once by
//! [`crate::prompt::subagent::spawn_subagent_branch`] and never restated —
//! no sidecar role table, no pinned soul-name parse. Two readers derive
//! from this one home:
//!
//! - a parent naming the lifecycle event of a **returning child** (§6,
//!   [`crate::prompt::dispatch::child_result`]) reads the child's subject
//!   at its terminal ref;
//! - an agent resolving **its own** soul + toolset under `litany advance`
//!   (§6 role-aware resolution, [`crate::prompt::resolve`]) reads its own.
//!
//! A *root* agent's dispatch commit subject is `step 001: dispatch [<id>]`
//! ([`crate::prompt::dispatch::step_commit`]) — it lacks the `dispatch:
//! <role>` prefix, so [`derive`] yields `None` for a root and the caller
//! applies the worker default (roots are workers).

use crate::prompt::Error;
use crate::template::GitRunner;
use std::path::Path;

/// Open-set role validity (§4.3) — the one home the dispatch built-in
/// and the `litany dispatch` CLI both consult before spawning.
pub mod validate;

/// Subject prefix of a child's dispatch commit (§2.5).
const DISPATCH_PREFIX: &str = "dispatch: ";

/// Derive the role recorded in the dispatch commit that names `agent_id`
/// (`dispatch: <role> [<agent-id>]`), reachable from `start` (a branch
/// ref or commit sha) and read in `dir` (any checkout onto the workspace
/// object store, §2.2). `None` when no such commit exists — a root, whose
/// dispatch subject lacks the prefix.
///
/// The `--grep` regex is anchored on the exact `[<agent-id>]` tail, so
/// only the agent's *own* dispatch commit matches — never a descendant's
/// `[<agent-id>-<sub>]`, whose bracket content differs. Exactly one
/// dispatch commit per branch carries this subject, so `-n 1` is the whole
/// answer; an unmatched search prints nothing and reads as `None`.
pub fn derive(
    dir: &Path,
    start: &str,
    agent_id: &str,
    git: &dyn GitRunner,
) -> Result<Option<String>, Error> {
    let pattern = format!("^dispatch: .+ \\[{agent_id}\\]$");
    let subject = git
        .run_capture(
            dir,
            &[
                "log",
                "-n",
                "1",
                "--format=%s",
                "-E",
                "--grep",
                pattern.as_str(),
                start,
            ],
        )
        .map_err(|source| Error::Git {
            op: "role derive log",
            source,
        })?;
    Ok(parse_role(subject.trim()))
}

/// One anchored `-E` pattern matching the **dispatch commit** that founds
/// `agent_id`'s branch — the single home of that question, so every reader
/// of "where does this branch begin" greps the same regex and they cannot
/// drift apart ([`founding_sha`] here, [`crate::prompt::compactor`]'s
/// checkpoint clock, which ORs it with its landing subjects).
///
/// **One pattern founds every branch.** A child's subject is `dispatch:
/// <role> [<id>]` and a root's is `step 001: dispatch [<id>]`
/// ([`crate::prompt::dispatch::step_commit`]), so the alternation covers
/// both and the root is the general path rather than a second case. The
/// two spellings are matched *exactly* rather than by the `[<id>]` tail
/// alone, because the executor's own transcript commits end in that tail
/// too (`transcript NNN: <origin> [<id>]`, and the stray recovery's
/// `transcript: recover delivered stray [<id>]`) and would otherwise
/// answer as the branch's founding.
pub fn founding_pattern(agent_id: &str) -> String {
    format!("^(dispatch: .+|step 001: dispatch) \\[{agent_id}\\]$")
}

/// Sha of the **dispatch commit** that founds `agent_id`'s branch,
/// reachable from `start` — the same single-home subject [`derive`]
/// parses, read as a commit rather than a role, matched by
/// [`founding_pattern`]. Two consumers derive from it: the compaction
/// landing takes the **compaction point** as its parent (ARCH §2.6 — the
/// commit the compactor forked off), and the retarget landing re-mints it
/// onto the target config commit (§2.2). `None` when no dispatch commit
/// matches — not an agent ref at all.
pub fn founding_sha(
    dir: &Path,
    start: &str,
    agent_id: &str,
    git: &dyn GitRunner,
) -> Result<Option<String>, Error> {
    let pattern = founding_pattern(agent_id);
    let sha = git
        .run_capture(
            dir,
            &[
                "log",
                "-n",
                "1",
                "--format=%H",
                "-E",
                "--grep",
                pattern.as_str(),
                start,
            ],
        )
        .map_err(|source| Error::Git {
            op: "founding sha log",
            source,
        })?;
    let sha = sha.trim();
    Ok((!sha.is_empty()).then(|| sha.to_string()))
}

/// Parse `<role>` out of a `dispatch: <role> [<id>]` subject, or `None`
/// when the subject is empty or is not a dispatch commit (no prefix).
fn parse_role(subject: &str) -> Option<String> {
    subject
        .strip_prefix(DISPATCH_PREFIX)?
        .split_whitespace()
        .next()
        .map(str::to_string)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;
    use std::io;

    /// A `GitRunner` whose `run_capture` returns a scripted subject and
    /// records the argv it was asked to run.
    struct SubjGit {
        subject: String,
        args: RefCell<Vec<String>>,
        fail: bool,
    }
    impl SubjGit {
        fn returning(subject: &str) -> Self {
            Self {
                subject: subject.to_string(),
                args: RefCell::new(Vec::new()),
                fail: false,
            }
        }
    }
    impl GitRunner for SubjGit {
        fn run(&self, _dest: &Path, _args: &[&str]) -> io::Result<()> {
            unreachable!("role derive only captures")
        }
        fn run_capture(&self, _dest: &Path, args: &[&str]) -> io::Result<String> {
            *self.args.borrow_mut() = args.iter().map(|s| s.to_string()).collect();
            if self.fail {
                Err(io::Error::other("log boom"))
            } else {
                Ok(self.subject.clone())
            }
        }
    }

    #[test]
    fn parse_role_reads_the_role_token() {
        assert_eq!(
            parse_role("dispatch: compactor [a-b]").as_deref(),
            Some("compactor")
        );
        assert_eq!(
            parse_role("dispatch: worker [a-b-c]").as_deref(),
            Some("worker")
        );
    }

    #[test]
    fn parse_role_is_none_for_a_root_or_empty_subject() {
        assert_eq!(parse_role("step 001: dispatch [a-b]"), None);
        assert_eq!(parse_role(""), None);
    }

    #[test]
    fn derive_reads_the_dispatch_subject_and_anchors_the_id() {
        let git = SubjGit::returning("dispatch: verifier [p-1-c-2]\n");
        let role = derive(Path::new("/wt"), "termref", "p-1-c-2", &git).unwrap();
        assert_eq!(role.as_deref(), Some("verifier"));
        let args = git.args.borrow();
        assert!(
            args.iter().any(|a| a == "^dispatch: .+ \\[p-1-c-2\\]$"),
            "{args:?}"
        );
        assert!(args.iter().any(|a| a == "termref"));
    }

    #[test]
    fn derive_is_none_when_no_dispatch_commit_matches() {
        let git = SubjGit::returning("");
        assert_eq!(derive(Path::new("/wt"), "ref", "a-b", &git).unwrap(), None);
    }

    #[test]
    fn derive_surfaces_a_git_failure() {
        let mut git = SubjGit::returning("");
        git.fail = true;
        let err = derive(Path::new("/wt"), "ref", "a-b", &git).unwrap_err();
        assert!(
            matches!(
                err,
                Error::Git {
                    op: "role derive log",
                    ..
                }
            ),
            "{err:?}"
        );
    }
}
