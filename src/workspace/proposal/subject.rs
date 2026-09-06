//! The listing's **SUBJECT** cell (`docs/DESIGN_LEARNING_LOOP.md` §3).
//!
//! A proposal commit's message is the reviewer's terminal response, and
//! its first line is therefore whatever the model happened to open with.
//! `template/souls/reviewer.md` asks for a one-line subject and the
//! models do not always give one: on a three-proposal listing, two opened
//! with a markdown heading (`## Final Response`) and a horizontal rule
//! (`---`), and the list a person reads before changing the policy every
//! conversation in the workspace resolves told them nothing about either
//! (bl-4c53). The soul instruction stays — it is what produces a *good*
//! subject — but a listing cannot rest on a model having complied.
//!
//! So the cell is derived rather than taken: the first line of the
//! message that **reads as a subject**, and the paths the proposal
//! changes when no line does — the fallback a git tool makes, and a
//! statement of what changes, which is what a subject in a diff list is
//! for. Ornament is what a subject is not: a blank line, a markdown
//! heading, a fence, a horizontal rule. Nothing here decides what a
//! *good* subject is; it only skips the lines that are not one.

use super::ops::{Error, capture};
use crate::template::GitRunner;
use std::path::Path;

/// The SUBJECT cell for the proposal at `target`, parented on `parent`.
/// The whole message is read (`%B`, not `%s`) because the first line is
/// exactly the one that may be ornament, and the diff is asked for only
/// when the message answers nothing.
pub(super) fn of(
    ws: &Path,
    parent: &str,
    target: &str,
    git: &dyn GitRunner,
) -> Result<String, Error> {
    let message = capture(ws, &["log", "-1", "--format=%B", target], "log body", git)?;
    match from_message(&message) {
        Some(subject) => Ok(subject),
        None => {
            let paths = capture(
                ws,
                &["diff", "--name-only", parent, target],
                "diff --name-only",
                git,
            )?;
            Ok(from_paths(&paths))
        }
    }
}

/// The first line of `message` that reads as a subject, or `None` when
/// every line is ornament (or there are none).
fn from_message(message: &str) -> Option<String> {
    message
        .lines()
        .map(str::trim)
        .find(|line| is_subject(line))
        .map(str::to_owned)
}

/// Is this trimmed line a subject? Everything that is not ornament —
/// blank, a markdown heading, a code fence, or a horizontal rule (a line
/// of nothing but rule characters). A test for what a line *is not*,
/// deliberately: judging whether prose makes a good subject is the
/// reviewer's job and its soul says so.
fn is_subject(line: &str) -> bool {
    !line.is_empty()
        && !line.starts_with('#')
        && !line.starts_with("```")
        && !line.chars().all(|c| matches!(c, '-' | '*' | '_' | '='))
}

/// The changed paths as the cell, in `git diff --name-only` order — the
/// pool rendering every other list in this crate uses, so a proposal
/// whose diff is somehow empty reads `(none)` rather than blank.
fn from_paths(paths: &str) -> String {
    let paths: Vec<&str> = paths
        .lines()
        .map(str::trim)
        .filter(|p| !p.is_empty())
        .collect();
    crate::name::pool(&paths)
}

#[cfg(test)]
mod tests {
    use super::{from_message, from_paths};

    #[test]
    fn the_first_line_is_the_subject_when_it_is_one() {
        assert_eq!(
            from_message("notes: record the lesson\n\nbecause x\n").as_deref(),
            Some("notes: record the lesson")
        );
    }

    // bl-4c53, verbatim from the listing that filed it: two of three
    // proposals opened with ornament and listed as `## Final Response`
    // and `---`.
    #[test]
    fn ornament_is_skipped_to_the_first_real_line() {
        for message in [
            "## Final Response\n\nskills/notes: record the correction\n",
            "---\n\nskills/notes: record the correction\n",
            "```\nskills/notes: record the correction\n```\n",
            "\n\n  skills/notes: record the correction  \n",
        ] {
            assert_eq!(
                from_message(message).as_deref(),
                Some("skills/notes: record the correction"),
                "{message:?}"
            );
        }
    }

    #[test]
    fn a_message_that_is_only_ornament_answers_nothing() {
        for message in ["", "\n\n", "## Final Response\n***\n===\n"] {
            assert_eq!(from_message(message), None, "{message:?}");
        }
    }

    #[test]
    fn the_fallback_is_what_the_proposal_changes() {
        assert_eq!(
            from_paths("skills/notes/SKILL.md\nfacts.md\n"),
            "skills/notes/SKILL.md, facts.md"
        );
        assert_eq!(from_paths("\n"), "(none)");
    }
}
