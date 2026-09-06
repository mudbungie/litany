//! `remember` built-in (`docs/DESIGN_CONTEXT_ECONOMY.md` §3,
//! `docs/DESIGN_LEARNING_LOOP.md` §3) — **the one lawful door from a
//! step to a durable fact**.
//!
//! Stdin is the `tool_use.input` block as JSON: `{ "fact": <string> }`.
//! Stdout is `{ "status": …, "proposal": <agent id>, "accept": <the
//! command an operator runs> }`.
//!
//! **Why it exists.** `facts.md` is the workspace's durable memory and
//! its only writers were `litany config` and an accepted reviewer
//! proposal — both the operator's. An agent asked to remember something
//! therefore had no way to, and nothing told it so: on a measured drive
//! (bl-3c11) a model spent about forty tool calls and four dispatched
//! subagents locating the file, then wrote a shell script, exported it
//! as `$EDITOR` and drove `litany config` — a control-plane write the
//! learning loop exists to keep out of an agent's hands, and one that
//! is now refused outright ([`crate::lineage`], bl-d273). A refusal
//! without a door leaves an agent with no answer at all, so the two are
//! one change: **the only writable thing from inside a step is the
//! facts file, through this door, and what it writes is a proposal.**
//!
//! **It writes no lineage.** The fact is appended to `facts.md` in one
//! config commit on `proposal/<agent-id>` — the branch no lineage
//! points at until `litany proposal <workspace> <agent-id> --accept`
//! fast-forwards it (§3 there). So the operator keeps the veto the
//! staged-proposal design is built on, the agent gets a truthful answer
//! to "remember this", and no prompt prefix moves under a running
//! conversation.
//!
//! **One proposal per agent, accumulating.** A second `remember`
//! *amends* the standing proposal rather than stacking a commit on it:
//! a proposal is one commit parented on the followed config commit, and
//! `litany proposal`'s freshness is that parent against the lineage head
//! ([`crate::template::authoring::Origin`] holds the reasoning). The
//! operator reads one proposal, whole.
//!
//! **A fact already recorded is `already_recorded`, at no cost.** The
//! append leaves the tree identical, which is the authoring routine's
//! own declined pass — the general path with an empty edit, not an arm
//! of its own. On the first `remember` the declined pass also deletes
//! the branch it created, so a duplicate proposes nothing rather than
//! staging an empty proposal.
//!
//! **The cap is the artifact's** ([`crate::facts::MAX_BYTES`]): the
//! authoring routine refuses an over-cap `facts.md` and the refusal
//! reaches the model verbatim, naming the size and the cap. Nothing is
//! staged, and a proposal that already stood is untouched.
//!
//! The calling agent's workspace and branch arrive via
//! `LITANY_CONV_REPO` / `LITANY_CONV_BRANCH` (§3.3, harness-derived),
//! never from model input — an agent can only ever propose against its
//! own lineage.

use serde::{Deserialize, Serialize};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use thiserror::Error;

use super::super::{ENV_CONV_BRANCH, ENV_CONV_REPO};
use super::dispatch::EnvLookup;
use super::harness;
use crate::template::authoring::{self, Origin, Pass};
use crate::template::{GitRunner, RealGit};

/// How much of the fact the commit subject carries before it is elided —
/// git's own soft subject limit, so `litany proposal`'s listing reads as
/// a table rather than wrapping.
const SUBJECT_BYTES: usize = 64;

/// Wire shape of the input. `deny_unknown_fields` so a malformed
/// `tool_use.input` surfaces as [`Error::InvalidJson`] rather than a
/// silent drop.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Input {
    fact: String,
}

/// Wire shape of the output. `accept` is the whole command an operator
/// runs, spelled out: the model's answer to "tell me exactly what you
/// used and where it is stored" is this line, and a model that has to
/// compose it from three facts will compose it wrong.
#[derive(Debug, Serialize, PartialEq, Eq)]
struct Output {
    status: &'static str,
    proposal: String,
    accept: String,
}

/// Every way [`run`] can fail. Each prints its own stderr message; per
/// §3.3 stderr concatenates after stdout into `tool_result.content` on a
/// non-zero exit, so the model reads the decline verbatim.
#[derive(Debug, Error)]
pub enum Error {
    #[error("invalid input JSON: {0}")]
    InvalidJson(#[source] serde_json::Error),
    #[error("read input from stdin: {0}")]
    StdinRead(#[source] io::Error),
    #[error("missing env var {0:?} (set by the harness per ARCH §3.3)")]
    MissingEnv(&'static str),
    /// An empty `fact` — declined, never recorded (PRINCIPLES *Decline
    /// illegal operations*): a blank line in `facts.md` spends the cap
    /// and says nothing, and the model meant to say something.
    #[error("`fact` is empty: say the fact you want a later conversation to know")]
    Blank,
    #[error("resolve the harness root: {0}")]
    Root(#[source] crate::harness_root::Error),
    #[error("resolve the followed config commit: {0}")]
    Lineage(#[source] io::Error),
    /// The authoring pass refused or failed — including the facts cap,
    /// in the routine's own voice.
    #[error(transparent)]
    Stage(#[from] authoring::Error),
    #[error("write to stdout: {0}")]
    Write(#[source] io::Error),
}

/// Production entry point invoked by `litany tool remember`.
pub fn run<R: Read, W: Write>(
    stdin: &mut R,
    stdout: &mut W,
    env: &dyn EnvLookup,
) -> Result<(), Error> {
    run_with(stdin, stdout, env, &RealGit::new())
}

/// [`run`] with the git runner injected.
pub fn run_with<R: Read, W: Write>(
    stdin: &mut R,
    stdout: &mut W,
    env: &dyn EnvLookup,
    git: &dyn GitRunner,
) -> Result<(), Error> {
    let mut buf = Vec::new();
    stdin.read_to_end(&mut buf).map_err(Error::StdinRead)?;
    let input: Input = serde_json::from_slice(&buf).map_err(Error::InvalidJson)?;
    let fact = input.fact.trim().to_owned();
    if fact.is_empty() {
        return Err(Error::Blank);
    }
    let repo = env
        .get(ENV_CONV_REPO)
        .ok_or(Error::MissingEnv(ENV_CONV_REPO))?;
    let agent = env
        .get(ENV_CONV_BRANCH)
        .and_then(|v| v.into_string().ok())
        .ok_or(Error::MissingEnv(ENV_CONV_BRANCH))?;
    let workspace = PathBuf::from(repo);
    let roots = harness::roots(env).map_err(Error::Root)?;
    let parent = harness::followed_commit(&workspace, &agent, git).map_err(Error::Lineage)?;

    let message = commit_message(&fact, &agent);
    let pass = authoring::author(
        &workspace,
        &roots.data,
        &agent,
        Origin::Proposal {
            parent: &parent,
            message: &message,
        },
        |checkout| append(checkout, &fact),
        git,
    )?;
    emit(stdout, &agent, &pass)
}

/// Append `fact` as its own paragraph of the checkout's `facts.md`,
/// creating the file when the lineage has authored none — the general
/// path with an empty input, not a case. One blank line separates
/// paragraphs, which is also what makes "is this already recorded?" a
/// question about the file rather than a substring search: a fact is a
/// paragraph, and it is already recorded when one of them is it.
///
/// **A fact already there writes nothing at all** — not the same bytes,
/// *nothing* — so the pass sees an untouched tree and declines (module
/// docs). Rewriting it normalized would count as a change on a file
/// whose trailing whitespace an operator's own edit had left different.
fn append(checkout: &Path, fact: &str) -> io::Result<()> {
    let path = checkout.join(crate::facts::FILE);
    let standing = std::fs::read_to_string(&path).unwrap_or_default();
    let trimmed = standing.trim_end();
    if trimmed.split("\n\n").any(|para| para.trim() == fact) {
        return Ok(());
    }
    let body = if trimmed.is_empty() {
        format!("{fact}\n")
    } else {
        format!("{trimmed}\n\n{fact}\n")
    };
    std::fs::write(path, body)
}

/// The proposal's commit message: the fact as the subject an operator
/// reads in `litany proposal`'s listing, and who proposed it. Elided on
/// a character boundary, so a multi-byte fact cannot split one.
fn commit_message(fact: &str, agent: &str) -> String {
    let head = fact.lines().next().unwrap_or_default();
    let subject = match head.char_indices().nth(SUBJECT_BYTES) {
        Some((cut, _)) => format!("{}…", &head[..cut]),
        None => head.to_owned(),
    };
    format!("facts: {subject}\n\nProposed by {agent} through the `remember` tool.\n")
}

/// Serialize the result (§3.3). `already_recorded` is the declined pass:
/// the fact was already in the lineage's `facts.md`, or already in this
/// agent's standing proposal.
fn emit<W: Write>(stdout: &mut W, agent: &str, pass: &Pass) -> Result<(), Error> {
    let payload = Output {
        status: match pass {
            Pass::Landed => "proposed",
            Pass::Declined { .. } => "already_recorded",
        },
        proposal: agent.to_owned(),
        accept: format!("litany proposal <workspace> {agent} --accept"),
    };
    let bytes = serde_json::to_vec(&payload).expect("Output is always serializable");
    stdout.write_all(&bytes).map_err(Error::Write)
}

#[cfg(test)]
mod tests;
