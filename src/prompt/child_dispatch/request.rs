//! The child-dispatch input shape ([`super::run`]'s one argument
//! struct), split from the primitive so each file stays under the
//! repo's 300-line cap.

use std::path::Path;

/// Inputs to a child dispatch. Built the same way by the dispatch
/// built-in and the `litany dispatch` CLI regardless of the target role.
pub struct ChildDispatchRequest<'a> {
    /// Workspace repository root. Used for the child's worktree path
    /// (sibling under `agents/`, §2.2), for soul resolution
    /// (`souls/<role>.md` in the governing config commit of the ref the
    /// child forks off), and as the deposit target's workspace.
    pub repo: &'a Path,
    /// Dispatching branch name — the parent's full hyphenated descent
    /// (§2.3). The child forks off this branch's tip, its id is
    /// `<parent>-<sub-id>`, and it is the deposit's sender (§2.11
    /// provenance) and the derived return address (§2.6).
    pub parent_branch: &'a str,
    /// The dispatching branch's worktree — where `git worktree add` runs
    /// (any access point onto the one workspace repository, §2.2).
    pub parent_worktree: &'a Path,
    /// The child's role (§2.5, §4.3): selects the pinned soul and labels
    /// the dispatch commit. `worker` for an ordinary child, `compactor`
    /// for a compaction dispatch (§2.7) — parent/child is provenance,
    /// the role is what the child *is*.
    pub role: &'a str,
    /// The goal / dispatch message. Written verbatim to the child's
    /// `goal.md` and deposited as its first inbox message.
    pub goal: &'a str,
    /// The child's supplied **name**, if any (ARCH §2.3, §2.11): the
    /// display fact, committed to the child's `name` on its dispatch
    /// commit and immutable thereafter, exactly like the goal (§2.8).
    /// Settled at the pre-flight before the fork: `Some` is checked for
    /// availability, so a taken or malformed name leaves no branch
    /// behind; `None` — every harness-initiated dispatch (compactor,
    /// verifier), and any dispatcher that omits the parameter — mints a
    /// one-word name against the same living-names scan (yog bl-aca4).
    /// No fork ends nameless.
    pub name: Option<&'a str>,
    /// The ref the child forks off (ARCH §2.3). `None` is the ordinary
    /// child dispatch off the parent's tip (§2.5); `Some(ref)` forks off
    /// another — a verifier off the worker's terminal ref (§6 gate), or
    /// `litany dispatch --from <ref>` (§7.2) — while the child id stays
    /// `<parent>-<sub>` (return address unchanged). Either way the
    /// child's **governing config commit derives from this ref** (§2.2
    /// fork-back-in): its ancestry begins here.
    pub fork_point: Option<&'a str>,
    /// The **working directory** the caller seeded for the child (§3.3,
    /// `litany dispatch --cwd`), already resolved to an absolute
    /// directory by [`crate::workspace::cwd::resolve`]. Written to the
    /// child's own mark before the fork, so its first tool call already
    /// runs there. `None` — every harness-initiated dispatch, the
    /// `dispatch` built-in, and any caller that omits the flag — leaves
    /// the mark unset, which is the general path with the fact absent:
    /// the child works in its worktree. Nothing inherits it; a mark is
    /// keyed by agent id and the parent's is never consulted here.
    pub cwd: Option<&'a Path>,
    /// Caller-supplied pinned documents (§2.5,
    /// [`crate::prompt::pinned_doc`]): exact bytes the dispatch commit
    /// snapshots at their validated destinations beside `goal.md` and
    /// `soul.md`, frozen before the child's first model request and
    /// carried to descendants by ordinary fork inheritance. Every
    /// harness-initiated dispatch passes
    /// [`crate::prompt::PinnedDocs::none`] — the general path with
    /// empty inputs.
    pub pins: &'a crate::prompt::PinnedDocs,
}
