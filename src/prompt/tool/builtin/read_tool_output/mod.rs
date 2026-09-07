//! `read_tool_output` built-in (ARCH §3.3 *Paging a cut capture*,
//! `docs/DESIGN_CONTEXT_ECONOMY.md` §7) — the page read that makes the
//! bounded projection's cut middle recoverable.
//!
//! **The capture is the page store.** The §3.3 bounded projection cuts
//! each captured stream to `head_bytes + tail_bytes` and names the
//! record the full bytes are in; until this tool that pointer led
//! outside every worktree, so the model's only recoveries were `bash` on
//! the engine's own box and "re-run with a filter" — advice §3.3 itself
//! undercuts, since "tool outputs are nondeterministic and cannot be
//! reconstructed by re-running". The bytes were never lost; they were
//! unaddressable. This tool addresses them, and adds no store to do it:
//! no index, no database, no second file — the [`address`] IS the record
//! path plus an offset.
//!
//! **The domain bound is the address's own agent-id segment.** A read is
//! refused unless that segment equals the caller's `LITANY_CONV_BRANCH`,
//! so an agent can page its own captures and cannot name anyone else's.
//! The check is one string equality on a component the grammar cannot
//! omit ([`address`]), which is why there is nothing to keep in step.
//!
//! **What this does to the diagnostic-only contract** (§2.3): exactly
//! one file becomes readable at runtime, `tools/<tool-id>/output.json`,
//! and only through a tool call the agent makes, landing in the
//! transcript as an ordinary tool result. `input.json`, `request.json`,
//! `response.json`, `stderr.log` and `driver.log` are still read by
//! nobody, no *decision* anywhere turns on a record's content, and no
//! assembly path touches `steps/` at all (§5.1 — the location still
//! makes that physically impossible).
//!
//! Stdin is `{"address": "<address>"}`; stdout is the page's bytes
//! verbatim, and the banner rides stderr in `read_file`'s shipped voice
//! (`docs/DESIGN_CONTEXT_ECONOMY.md` §7.1), which the result envelope
//! surfaces on success too — so a page can be handed straight to a
//! patch or a diff without a header line of ours inside what the model
//! believes the tool said.

pub(crate) mod address;
mod error;

use super::super::ENV_CONV_BRANCH;
use super::super::ENV_CONV_REPO;
use super::super::ToolOutputRecord;
use super::dispatch::EnvLookup;
use address::Address;
pub use error::Error;
use serde::Deserialize;
use std::io::{Read, Write};
use std::path::Path;

/// Bytes one page returns. The shipped `tool_output:` allowance
/// (`docs/DESIGN_CONTEXT_ECONOMY.md` §7.1: 2048 + 2048), so a page rides
/// the very projection that cut the result it answers **untouched** — a
/// stream within `head_bytes + tail_bytes` is passed through
/// marker-free (`prompt::tool::bound`). The banner is on stderr for the
/// same reason: the streams are bounded independently, so a page that is
/// exactly the allowance stays exactly the allowance. A workspace that
/// narrows the policy below this gets its pages cut like any other
/// result — which is what a severable policy block is for.
const PAGE_BYTES: usize = 4096;

/// Wire shape of the input. One field, because the address carries
/// every discriminant a read needs — a `stream:` or `offset:` beside it
/// would be a second home for a fact the address already states.
/// `deny_unknown_fields` keeps a third knob from being silently dropped.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Input {
    address: String,
}

/// Read the address from `stdin`, answer it out of the calling agent's
/// own capture, and write the page to `stdout` and its banner to
/// `stderr`.
pub fn run<R: Read, W: Write, E: Write>(
    stdin: &mut R,
    stdout: &mut W,
    stderr: &mut E,
    env: &dyn EnvLookup,
) -> Result<(), Error> {
    let mut buf = Vec::new();
    stdin.read_to_end(&mut buf).map_err(Error::StdinRead)?;
    let input: Input = serde_json::from_slice(&buf).map_err(Error::InvalidJson)?;
    let repo = env
        .get(ENV_CONV_REPO)
        .ok_or(Error::MissingEnv(ENV_CONV_REPO))?;
    // The agent id is ASCII by construction (§2.3 — a hyphenated
    // descent of minted words), so the lossy read is total here rather
    // than approximate.
    let caller = env
        .get(ENV_CONV_BRANCH)
        .ok_or(Error::MissingEnv(ENV_CONV_BRANCH))?
        .to_string_lossy()
        .into_owned();
    let address = Address::parse(&input.address).ok_or_else(|| Error::Malformed {
        given: input.address.clone(),
    })?;
    if address.agent_id != caller {
        return Err(Error::ForeignAgent {
            theirs: address.agent_id,
            ours: caller,
        });
    }
    let record = Path::new(&repo).join(address.record());
    let bytes = std::fs::read(&record).map_err(|source| Error::NoRecord {
        record: record.clone(),
        source,
    })?;
    let captured: ToolOutputRecord =
        serde_json::from_slice(&bytes).map_err(|source| Error::Unreadable { record, source })?;
    let stream = match address.stream.as_str() {
        "stderr" => captured.stderr,
        _ => captured.stdout,
    };
    let page = page(stream.as_bytes(), address.offset).ok_or_else(|| Error::PastEnd {
        offset: address.offset,
        stream: address.stream.clone(),
        total: stream.len(),
    })?;
    stdout.write_all(page).map_err(Error::Write)?;
    let end = address.offset.saturating_add(page.len());
    stderr
        .write_all(banner(&address, end, stream.len()).as_bytes())
        .map_err(Error::Write)
}

/// The page's bytes: [`PAGE_BYTES`] from `offset`, or whatever is left.
/// `None` past the end — an offset **at** the end is the empty final
/// page, the general path with nothing remaining rather than an arm of
/// its own.
fn page(stream: &[u8], offset: usize) -> Option<&[u8]> {
    let end = offset.saturating_add(PAGE_BYTES).min(stream.len());
    stream.get(offset..end)
}

/// The one line the read writes to stderr, in `read_file`'s shipped
/// voice (`docs/DESIGN_CONTEXT_ECONOMY.md` §7.1: *"read_file: 200 of 843
/// lines from offset 1; continue with offset 201"*). One shape for every
/// case, with the trailing clause naming the next address when bytes
/// remain and saying so outright when none do — an *absent* clause is
/// the ambiguity §7.1 blames for invented bounds, and this tool exists
/// to be paged to the end.
fn banner(address: &Address, end: usize, total: usize) -> String {
    let (offset, stream, record) = (address.offset, &address.stream, address.record());
    let rest = if end < total {
        format!("continue with address {}", address.at(end))
    } else {
        "end of stream".to_string()
    };
    format!("read_tool_output: bytes {offset}-{end} of {total} from {record} {stream}; {rest}\n")
}

#[cfg(test)]
mod tests;
#[cfg(test)]
mod tests_address;
