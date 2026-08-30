//! The agent **name** — the one home of an agent's display identity
//! (ARCH §2.1, §2.3, §2.11).
//!
//! A **name** is a human-spoken discriminator for an agent: settled at
//! the dispatch that creates the agent — supplied by the dispatcher, or
//! minted from the embedded wordlist on omission ([`mint`], the yog
//! bl-aca4 ruling; two words in PascalCase since bl-79a2) — and
//! immutable thereafter exactly like the goal
//! (§2.8). Unnamed is a *readable* state (pre-mint stock, until
//! retention ages it out), not a creatable one. The **id** stays the
//! *identifier* — branch name, worktree directory, `steps/` and `inbox/`
//! namespace keys (§2.2, §2.3) — and never carries display semantics;
//! the name never addresses a path.
//!
//! **One home: a `name` file on the agent's own dispatch commit**, beside
//! `goal.md` (§2.3 step 2). Everything else derives from it:
//!
//! - the `agents/*` ref namespace stays the workspace's *only* registry
//!   (§2.3) — reading a name is `git show agents/<id>:name`, a query, not
//!   a stored index, so no workspace-root registry file exists to drift
//!   (`docs/PRINCIPLES.md` Single source of truth);
//! - worktree teardown (§2.3 step 6) cannot lose it: it is a commit;
//! - retention (ref deletion, §9.2) recycles the name with no cleanup
//!   path at all — the ref goes, the blob goes, the name is free.
//!
//! **Every dispatch commit writes the file; empty content means
//! unnamed.** Writing it only for a named agent would make absence a
//! second shape, and absence would have to be *established* rather than
//! merely left alone: a child forks off its parent's tip (§2.5) and so
//! inherits the parent's `name` blob, so the dispatch commit would have
//! to delete it — and that deletion then rides the §2.6 work-product
//! transfer and the §2.7 compaction landing straight back into the parent
//! and unnames it. One always-written file dissolves all three cases at
//! once: like `goal.md` and `soul.md` the name is overwritten in place at
//! the dispatch commit and frozen thereafter (§2.3 *Goal and soul are
//! pinned files*), so it is always a rewrite and never a deletion.
//!
//! **Name space and id space are disjoint by construction.** Every agent
//! id begins with a compact `YYYYMMDDTHHMMSSZ` timestamp (§2.3,
//! `prompt::clock`), so a name beginning with one is refused at creation.
//! [`resolve`] therefore never *guesses* which reading of a needle was
//! meant — a needle is an id or it is a name, never both — and the only
//! ambiguity left to refuse is one name worn by two living agents.

use super::{GitRunner, Path, agent_exists, agent_ids, agent_ref, repo_git};
use std::io;

pub mod mint;

/// Worktree-relative path of the name fact, committed on the dispatch
/// commit beside `goal.md` (§2.3 step 2). At the worktree root for the
/// same reason `goal.md` is.
pub const NAME_FILE: &str = "name";

/// Why a requested name may not be given to a new agent. Rendered inside
/// the verb's uniform `litany <verb>: <error>` failure line (§3.4).
#[derive(Debug, thiserror::Error)]
pub enum Unavailable {
    /// Not one unbroken word: the same single-path-component rule agent
    /// ids answer to ([`crate::name::is_component`]), plus no whitespace —
    /// the stored fact is one line of a file, and a name that cannot
    /// round-trip through it is declined, never munged (PRINCIPLES
    /// "Decline illegal operations").
    #[error(
        "agent name {0:?} is not a single unbroken word — a name is one path component with \
         no whitespace (ARCH §2.3); pick a name like `pale-otter`"
    )]
    Malformed(String),
    /// Reads as an agent id, which would put the two spaces back in
    /// contact and make [`resolve`] a guess (module docs).
    #[error(
        "agent name {0:?} starts with an agent-id timestamp (`YYYYMMDDTHHMMSSZ`) — names and \
         ids are disjoint spaces so addressing never has to guess (ARCH §2.3); pick a name \
         that does not begin like an id"
    )]
    IdShaped(String),
    /// Already worn by a living agent. Uniqueness is enforced where the
    /// fact lives — a scan of the `agents/*` refs (§2.3).
    #[error(
        "agent name {name:?} is already worn by agent {holder} in this workspace — a name is \
         unique among living agents (ARCH §2.3); pick another, or `litany delete` the holder \
         to recycle it"
    )]
    Taken { name: String, holder: String },
    /// The `agents/*` scan that enforces uniqueness could not be run.
    #[error("scan the workspace's agent names: {0}")]
    Scan(#[source] io::Error),
    /// The mint-on-omission path ([`mint::preflight`]) found every word
    /// of the embedded pool worn by a living agent — loud, never a loop.
    #[error(transparent)]
    Exhausted(#[from] mint::MintError),
}

/// A needle that names two or more living agents — refused, never
/// guessed (§2.11). The candidates travel with the refusal so the sender
/// can re-address by id without a second lookup.
#[derive(Debug, thiserror::Error)]
#[error(
    "agent name {name:?} is ambiguous in this workspace — it is worn by {candidates}; \
     address the one you mean by its agent id (ARCH §2.11)"
)]
pub struct Ambiguous {
    name: String,
    candidates: String,
}

/// Is `s` the compact timestamp `YYYYMMDDTHHMMSSZ` every agent id begins
/// with (`prompt::clock::Clock::now_compact`)?
fn is_id_timestamp(s: &str) -> bool {
    let b = s.as_bytes();
    b.len() == 16
        && b[8] == b'T'
        && b[15] == b'Z'
        && b[..8].iter().all(u8::is_ascii_digit)
        && b[9..15].iter().all(u8::is_ascii_digit)
}

/// The name `agent_id` wears, or `None` for an unnamed agent — an empty
/// (or absent) `name` blob. `git show agents/<id>:name` against the bare
/// repo: the ref namespace is the registry, so this is a query (§2.3).
pub fn read(workspace: &Path, agent_id: &str, git: &dyn GitRunner) -> Option<String> {
    let spec = format!("{}:{NAME_FILE}", agent_ref(agent_id));
    let out = git
        .run_capture(&repo_git(workspace), &["show", spec.as_str()])
        .ok()?;
    (!out.is_empty()).then_some(out)
}

/// The name committed in `worktree`, or `None` for an unnamed agent —
/// the read **context assembly** makes (ARCH §2.3, §5.1). Same single
/// fact as [`read`], reached by the route each caller is entitled to:
/// [`read`] answers *about* an agent from outside, off the `agents/*`
/// registry, while the system slot is composed for the agent whose tree
/// is in hand and must stay a pure function of that tree, so it reads
/// the file (§5.1 — one input; replay resolves the same bytes). Two
/// queries on one home, never a second copy.
pub fn in_worktree(worktree: &Path) -> Option<String> {
    let body = std::fs::read_to_string(worktree.join(NAME_FILE)).ok()?;
    let name = body.trim();
    (!name.is_empty()).then(|| name.to_owned())
}

/// Every named agent in the workspace, as `(agent id, name)` — the
/// `agents/*` enumeration (§2.3) with each ref's name read out of its own
/// tree. Unnamed agents are simply absent: the general path with empty
/// inputs, no flag and no second listing.
pub fn named(workspace: &Path, git: &dyn GitRunner) -> io::Result<Vec<(String, String)>> {
    Ok(agent_ids(workspace, git)?
        .into_iter()
        .filter_map(|id| read(workspace, &id, git).map(|name| (id, name)))
        .collect())
}

/// May a new agent be created under `name`? Well-formedness first, then
/// uniqueness against the living agents — the pre-flight both fork paths
/// run *before* forking, beside the §6 budget gate and the §4.3 role
/// check, so a refusal leaves no branch, no worktree and no inbox.
pub fn require_available(
    workspace: &Path,
    name: &str,
    git: &dyn GitRunner,
) -> Result<(), Unavailable> {
    if !crate::name::is_component(name) || name.chars().any(char::is_whitespace) {
        return Err(Unavailable::Malformed(name.to_owned()));
    }
    if name.split('-').next().is_some_and(is_id_timestamp) {
        return Err(Unavailable::IdShaped(name.to_owned()));
    }
    match named(workspace, git)
        .map_err(Unavailable::Scan)?
        .into_iter()
        .find(|(_, worn)| worn == name)
    {
        Some((holder, _)) => Err(Unavailable::Taken {
            name: name.to_owned(),
            holder,
        }),
        None => Ok(()),
    }
}

/// Resolve an outside-supplied `needle` to an agent id: an exact id match
/// first, else the unique living agent wearing that name (§2.11).
///
/// Total but for ambiguity. A needle nothing answers to comes back
/// unchanged, so the caller's own existence guard
/// ([`super::require_agent`]) speaks the one "no such agent" decline
/// every id-taking verb shares — this adds a *reading*, not a second
/// voice. A workspace the scan cannot read likewise falls through, so the
/// layout guard ([`super::require`]) reports it rather than a raw git
/// failure surfacing from here.
pub fn resolve(workspace: &Path, needle: &str, git: &dyn GitRunner) -> Result<String, Ambiguous> {
    if agent_exists(workspace, needle, git) {
        return Ok(needle.to_owned());
    }
    let mut hits: Vec<String> = named(workspace, git)
        .unwrap_or_default()
        .into_iter()
        .filter(|(_, worn)| worn == needle)
        .map(|(id, _)| id)
        .collect();
    if hits.len() < 2 {
        return Ok(hits.pop().unwrap_or_else(|| needle.to_owned()));
    }
    hits.sort();
    Err(Ambiguous {
        name: needle.to_owned(),
        candidates: crate::name::pool(&hits),
    })
}

/// Settle the forked tree's name fact and stage it: write `name` — the
/// agent's own, or empty when it has none — and `git add` it. Part of the
/// dispatch commit's one staging act
/// ([`crate::prompt::dispatch::trim_to_context`]), so no fork path can
/// skip it and no fork ever keeps the name it inherited (module docs).
pub fn settle(
    worktree: &Path,
    name: Option<&str>,
    git: &dyn GitRunner,
) -> Result<(), std::io::Error> {
    let body = name.map_or_else(String::new, |n| format!("{n}\n"));
    std::fs::write(worktree.join(NAME_FILE), body)?;
    git.run(worktree, &["add", NAME_FILE])
}

#[cfg(test)]
mod tests;
