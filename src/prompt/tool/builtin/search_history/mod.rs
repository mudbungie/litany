//! `search_history` built-in (ARCH §3.3, `docs/DESIGN_CONTEXT_ECONOMY.md`
//! §4) — a read over the workspace's own `agents/*` refs that returns
//! stored transcript entries verbatim.
//!
//! **No second store.** git is the search surface: no index, no SQLite,
//! no FTS. Every entry an agent ever committed is a blob in the
//! workspace repository, so the search is one `git log` and the recovery
//! is one `git show`.
//!
//! **Why a tool and not a `bash` recipe** (§4): in the engine's own
//! process the recipe *is* one `git log`, but the deployment litany
//! ships into routes `bash` to a foot on another machine, where the
//! workspace repository does not exist. A tool whose subject is the
//! conversation is an engine act — `cd` and `load_skill` are the
//! precedent — and is the only way the search reaches the history
//! wherever the shell runs.
//!
//! Stdin is the `tool_use.input` JSON, exactly one of:
//!
//! - `{"pattern": "<text>"}` — a fixed-string pickaxe over every
//!   `agents/*` ref, restricted to the two entry directories. Each hit
//!   is the commit that **added** an entry, so a squash or a deletion is
//!   never a hit and a compactor's ref shares its parent's commits as
//!   one object walked once. Stdout lists every hit's `<commit>:<path>`
//!   address one per line, then the newest [`PREVIEW_COUNT`] entries
//!   framed and bounded head+tail ([`PREVIEW_BOUND`]) with the §3.3
//!   marker naming the address.
//! - `{"entry": "<commit>:<path>"}` — the recovery path: that one entry
//!   whole and byte-for-byte, subject only to the workflow's ordinary
//!   `tool_output` projection.
//!
//! Neither more nor fewer inputs (§4): a `limit`, a `scope` or a regex
//! flag is a knob the address already answers — narrow the pattern, or
//! read an address. Both inputs at once, or neither, is a decline.
//!
//! **The compactor's ref is what makes squashed spans findable** (§5.4):
//! the landing squashes the span on the dispatching branch but leaves
//! `agents/<id>-<compactor>` standing, and that ref is inside the
//! `agents/*` glob, so a pre-compaction entry is still reached — pinned
//! by `tests::a_pre_compaction_entry_is_found_through_the_compactors_ref`.

use serde::Deserialize;
use std::io::{self, Read, Write};
use std::path::Path;
use thiserror::Error;

use super::super::ENV_CONV_REPO;
use super::super::bound;
use super::dispatch::EnvLookup;
use crate::config::ToolOutputBound;
use crate::template::{GitRunner, RealGit};
use crate::workspace;

/// How many of the newest hits are previewed verbatim (§4). Five is the
/// design's number: enough to answer "what did we say about X" without
/// the listing becoming the context problem it exists to relieve.
const PREVIEW_COUNT: usize = 5;

/// The head+tail cut applied to each previewed entry (§4) — the one
/// bound litany already states in bytes (§3.3 bounded projection). The
/// omitted middle is replaced by the §3.3 marker, whose `full record:`
/// field names the entry's own address, so the model can follow it back
/// with `{entry}` and read the whole thing.
const PREVIEW_BOUND: ToolOutputBound = ToolOutputBound {
    head_bytes: 4096,
    tail_bytes: 4096,
};

/// The two directories a stored transcript entry lives in (§2.3):
/// `messages/NNN-<model-id>.json` and `summary/NNN.md`. They are the
/// pathspec the pickaxe is restricted to, so a work product that
/// happens to carry the pattern is not history and is never a hit.
const ENTRY_DIRS: [&str; 2] = ["messages", "summary"];

/// The ref glob the search walks: every agent branch of the workspace,
/// the compactor's soft archive (`agents/<id>-<compactor>`) included.
const AGENT_BRANCHES: &str = "--branches=agents/*";

/// Wire shape of the input. Both fields are optional here so the
/// exactly-one rule is a decline the model can read rather than a serde
/// message about a missing field; `deny_unknown_fields` keeps a third
/// knob from being silently dropped.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Input {
    pattern: Option<String>,
    entry: Option<String>,
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
    /// Neither `pattern` nor `entry`, or both. The contract is exactly
    /// one (§4) and the decline says which two shapes are legal.
    #[error(
        "give exactly one of {{\"pattern\": \"<text>\"}} (search) or \
         {{\"entry\": \"<commit>:<path>\"}} (recover one entry)"
    )]
    Ambiguous,
    /// A git query failed — an unreadable repository, or an `entry`
    /// address naming no blob. git's own stderr rides the message, so
    /// the model reads why.
    #[error("{op}: {source}")]
    Git {
        op: &'static str,
        #[source]
        source: io::Error,
    },
    #[error("write to stdout: {0}")]
    Write(#[source] io::Error),
}

/// One hit: the commit that added the entry, the entry's path, and the
/// blob it added there. The blob is the entry's **identity** — see
/// [`parse`] on why one entry can be added by two commits.
struct Hit {
    commit: String,
    path: String,
    blob: String,
}

impl Hit {
    /// The hit's **address** — the `<commit>:<path>` string that both
    /// names it in the listing and recovers it whole through `{entry}`.
    fn address(&self) -> String {
        format!("{}:{}", self.commit, self.path)
    }
}

/// Read the input from `stdin`, answer it out of the workspace's object
/// store, and write the bytes to `stdout`. There is no injected-git
/// variant: every arm here *is* a git query, so a stubbed runner would
/// test the stub — the tests drive real repositories instead.
pub fn run<R: Read, W: Write>(
    stdin: &mut R,
    stdout: &mut W,
    env: &dyn EnvLookup,
) -> Result<(), Error> {
    let git = &RealGit::new();
    let mut buf = Vec::new();
    stdin.read_to_end(&mut buf).map_err(Error::StdinRead)?;
    let input: Input = serde_json::from_slice(&buf).map_err(Error::InvalidJson)?;
    let repo = env
        .get(ENV_CONV_REPO)
        .ok_or(Error::MissingEnv(ENV_CONV_REPO))?;
    let repo = workspace::repo_git(Path::new(&repo));
    let out = match (input.pattern, input.entry) {
        (Some(pattern), None) => search(&repo, &pattern, git)?,
        (None, Some(address)) => blob(&repo, &address, git)?,
        _ => return Err(Error::Ambiguous),
    };
    stdout.write_all(&out).map_err(Error::Write)
}

/// The `{pattern}` answer: the hit listing, then the newest
/// [`PREVIEW_COUNT`] entries framed and bounded. No hit is a clean empty
/// listing — the general path with an empty result, not a decline.
fn search(repo: &Path, pattern: &str, git: &dyn GitRunner) -> Result<Vec<u8>, Error> {
    let pickaxe = format!("-S{pattern}");
    let mut args = vec![
        "log",
        AGENT_BRANCHES,
        "--diff-filter=A",
        "--format=%H",
        "--raw",
        "--no-abbrev",
        pickaxe.as_str(),
        "--",
    ];
    args.extend_from_slice(&ENTRY_DIRS);
    let log = git.run_capture(repo, &args).map_err(|source| Error::Git {
        op: "search_history log",
        source,
    })?;
    let hits = parse(&log);

    let mut out = Vec::new();
    for hit in &hits {
        out.extend_from_slice(hit.address().as_bytes());
        out.push(b'\n');
    }
    for hit in hits.iter().take(PREVIEW_COUNT) {
        let address = hit.address();
        let content = blob(repo, &address, git)?;
        let bounded = bound::apply(&content, "entry", Some(PREVIEW_BOUND), Path::new(&address));
        out.extend_from_slice(format!("\n<entry address=\"{address}\">\n").as_bytes());
        out.extend_from_slice(&bounded);
        out.extend_from_slice(b"\n</entry>\n");
    }
    Ok(out)
}

/// One entry's stored bytes, read out of the object store at `address`
/// (`<commit>:<path>`) — never from a summary, never re-rendered. An
/// address naming no blob is git's decline, surfaced verbatim.
fn blob(repo: &Path, address: &str, git: &dyn GitRunner) -> Result<Vec<u8>, Error> {
    git.run_capture_bytes(repo, &["show", address])
        .map_err(|source| Error::Git {
            op: "search_history show",
            source,
        })
}

/// Parse `git log --format=%H --raw --no-abbrev` output into hits,
/// newest first. A `:`-led raw line is one added file of the commit
/// named on the last bare line: `:<src mode> <dst mode> <src sha> <dst
/// sha> <status>\t<path>`, so the post-image sha is the fourth field
/// and the path follows the tab.
///
/// **One entry, one hit** (§4). The naive walk reports an entry once per
/// commit that added it, and a rebase-forward landing (§2.6) adds the
/// replayed tail a *second* time: the original commit stands on the
/// compactor's ref and the replayed copy on the dispatching branch, so
/// every surviving entry of a compacted branch would be listed twice
/// and the listing would double at each compaction. An entry's identity
/// is its path and its bytes, not the commit that happens to carry it,
/// so the first — newest, hence the live branch's copy rather than the
/// archive's — address for each `(path, blob)` is kept and the rest
/// dropped.
fn parse(log: &str) -> Vec<Hit> {
    let mut hits: Vec<Hit> = Vec::new();
    let mut commit = String::new();
    for line in log.lines().filter(|l| !l.trim().is_empty()) {
        let Some(raw) = line.strip_prefix(':') else {
            commit = line.trim().to_string();
            continue;
        };
        let Some((fields, path)) = raw.split_once('\t') else {
            continue;
        };
        let Some(blob) = fields.split_whitespace().nth(3) else {
            continue;
        };
        if hits.iter().any(|h| h.path == path && h.blob == blob) {
            continue;
        }
        hits.push(Hit {
            commit: commit.clone(),
            path: path.to_string(),
            blob: blob.to_string(),
        });
    }
    hits
}

#[cfg(test)]
mod tests;
