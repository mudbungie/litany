//! `python` built-in — run a model-authored **program** beside the
//! engine (`docs/DESIGN_CODE_EXECUTION.md` §2.2).
//!
//! Stdin is the `tool_use.input` block as JSON: `{ "program": <string> }`.
//! The built-in generates the program's [`stub`] module from the calling
//! role's effective toolset, runs `python3 -` with the program on stdin
//! and that module on `PYTHONPATH`, and relays the interpreter's stdout,
//! stderr and exit code — the ordinary stdio contract, nothing added.
//! **Only the program's stdout reaches the model**, through the ordinary
//! result envelope and its `tool_output:` bounding; the inner
//! invocations it composed enter no transcript, not a line and not a
//! tally, because the program's own output is the model's whole reading
//! of what happened (§2.4).
//!
//! The interpreter inherits this process's working directory — the
//! agent's own, resolved once by the executor (ARCH §3.3 *Working
//! directory*) — and its environment, which already carries the §3.3
//! contract vars the door verb reads back. Nothing about the door is
//! written into the worktree: the driver target and this invocation's
//! `tool_use.id` are baked into the generated module, and the module
//! lands beside this invocation's own record (§2.3), out of the tree.
//!
//! **No deadline** (§2.5). `bash` has none either: the bounds are
//! `litany stop` — the interpreter and every tool it spawned are in this
//! process's group and fall to the one SIGTERM ([`super::child`]) — and
//! the whole-tree budget (ARCH §6).
//!
//! **Nothing probes for python3** (§2.6). `bash` assumes `sh` and says
//! so in its definition; `python` assumes `python3` and says so in its.
//! An operator who lacks it does not grant the tool — the role's
//! `tools:` list is already the one home for "this deployment offers
//! this tool". A missing interpreter is therefore the same in-band
//! failure a missing binary is under `bash`: exit 127, stderr naming it.

use super::child;
use super::dispatch::EnvLookup;
use crate::prompt::dispatch::door::caller::{self, Caller};
use crate::prompt::dispatch::tools::{read_description, read_schema};
use crate::prompt::tool::{ENV_TOOL_ID, STEP_TOOLS_SUBDIR};
use serde::Deserialize;
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::process::Command;
use stub::ToolDef;
use thiserror::Error;

pub(crate) mod stub;

/// The interpreter, as the tool's definition names it (§2.6).
const INTERPRETER: &str = "python3";
/// The generated module's file name — the name a program imports.
const MODULE: &str = "litany_tools.py";
/// The exit code a missing interpreter answers with, the same code a
/// missing binary answers with under `bash` (§2.4).
const NOT_FOUND: i32 = 127;
/// The env var the module search path lives in.
const PYTHONPATH: &str = "PYTHONPATH";

/// Wire shape of the input. `deny_unknown_fields` so a malformed
/// `tool_use.input` surfaces as [`Error::InvalidJson`] rather than
/// silently dropping a field the model meant to pass.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Input {
    program: String,
}

/// Every way [`run`] can fail at the harness layer. A program that ran
/// and failed is never one of these: its traceback is on stderr and its
/// code rides back as the tool's exit code.
#[derive(Debug, Error)]
pub enum Error {
    /// Stdin handed back bytes that did not parse as the documented
    /// shape — wrong type, missing `program`, or extra fields.
    #[error("invalid input JSON: {0}")]
    InvalidJson(#[source] serde_json::Error),
    /// The harness's stdin pipe failed mid-read.
    #[error("read input from stdin: {0}")]
    StdinRead(#[source] io::Error),
    /// A contract env var the §3.3 stdio contract sets was absent.
    #[error("missing env var {0:?} (set by the harness per ARCH §3.3)")]
    MissingEnv(&'static str),
    /// The calling agent could not be resolved, so there is no toolset
    /// to generate a module from ([`caller`]).
    #[error(transparent)]
    Caller(#[from] caller::Error),
    /// A tool's committed schema or description could not be read.
    #[error(transparent)]
    Definitions(#[from] crate::prompt::Error),
    /// The stub module could not be written beside this invocation's
    /// record, so the program would import a module that is not there.
    #[error("write {path}: {source}")]
    Module {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    /// The interpreter could not be spawned or reaped ([`child::Error`]).
    /// A *missing* interpreter is not here — it is the in-band exit 127.
    #[error(transparent)]
    Child(#[from] child::Error),
    /// Writing the interpreter's stdout to the harness's stdout failed.
    #[error("write to stdout: {0}")]
    Stdout(#[source] io::Error),
    /// Same as [`Error::Stdout`] but for the stderr stream.
    #[error("write to stderr: {0}")]
    Stderr(#[source] io::Error),
}

/// Production entry point invoked by `litany tool python`. Installs the
/// SIGTERM forwarder once per process and delegates to [`run_with`].
pub fn run<R: Read, W: Write, E: Write>(
    stdin: &mut R,
    stdout: &mut W,
    stderr: &mut E,
    bindings: &super::Bindings<'_>,
    env: &dyn EnvLookup,
) -> Result<i32, Error> {
    child::install_sigterm_handler();
    run_with(stdin, stdout, stderr, bindings, env, INTERPRETER)
}

/// Same as [`run`] with the interpreter named, so a test can exercise
/// the missing-interpreter answer without scrubbing `PATH`.
pub(crate) fn run_with<R: Read, W: Write, E: Write>(
    stdin: &mut R,
    stdout: &mut W,
    stderr: &mut E,
    bindings: &super::Bindings<'_>,
    env: &dyn EnvLookup,
    interpreter: &str,
) -> Result<i32, Error> {
    let mut buf = Vec::new();
    stdin.read_to_end(&mut buf).map_err(Error::StdinRead)?;
    let input: Input = serde_json::from_slice(&buf).map_err(Error::InvalidJson)?;
    let tool_id = env
        .get(ENV_TOOL_ID)
        .ok_or(Error::MissingEnv(ENV_TOOL_ID))?
        .into_string()
        .map_err(|_| Error::MissingEnv(ENV_TOOL_ID))?;
    let caller = caller::resolve(
        env,
        bindings.driver_target,
        bindings.adapter_target,
        bindings.stop,
        bindings.injection,
    )?;

    let record = caller.step_dir.join(STEP_TOOLS_SUBDIR).join(&tool_id);
    let module = record.join(MODULE);
    let source = stub::module(&toolset(&caller)?, bindings.driver_target, &tool_id);
    write(&record, &module, &source)?;

    let mut cmd = Command::new(interpreter);
    cmd.arg("-").env(PYTHONPATH, path_with(env, &record));
    // The spawn's arguments are named so the call fits on one line:
    // exploded across argument lines, tarpaulin's llvm engine
    // mis-attributes one of them as uncovered (the quirk `builtin::run`
    // carries a `rustfmt::skip` for).
    let src = Some(input.program.as_bytes());
    let (stop, grace) = (bindings.stop, child::CASCADE_DEADLINE);
    let spawned = child::run(&mut cmd, src, stop, grace);
    let done = match spawned {
        Ok(done) => done,
        Err(child::Error::Spawn(e)) if e.kind() == io::ErrorKind::NotFound => {
            let text = format!(
                "{interpreter}: not found. The `python` tool runs a program with \
                 {interpreter} on this machine's PATH (ARCH §3.3); this deployment \
                 has none, so the tool should not be granted here.\n"
            );
            stderr.write_all(text.as_bytes()).map_err(Error::Stderr)?;
            return Ok(NOT_FOUND);
        }
        Err(e) => return Err(Error::Child(e)),
    };

    stdout.write_all(&done.stdout).map_err(Error::Stdout)?;
    stderr.write_all(&done.stderr).map_err(Error::Stderr)?;
    Ok(done.code)
}

/// The program's effective toolset, read where the door reads it (§2.7):
/// the role's grant, resolved against the definitions committed in the
/// agent's own worktree, plus everything injected into its requests,
/// which carries its own. `python` is absent from its own module (depth
/// 1, ARCH §3.3), and so is a granted name whose schema this branch does
/// not carry — availability is the intersection, exactly as the
/// composer's is ([`crate::prompt::dispatch::tools`]).
fn toolset(caller: &Caller) -> Result<Vec<ToolDef>, crate::prompt::Error> {
    let worktree = crate::workspace::agent_worktree(&caller.workspace, &caller.agent);
    let mut out = Vec::new();
    for name in caller.grant.iter().filter(|n| n.as_str() != super::PYTHON) {
        if let Some(input_schema) = read_schema(&worktree, name)? {
            out.push(ToolDef {
                name: name.clone(),
                description: read_description(&worktree, name)?,
                input_schema,
            });
        }
    }
    for tool in caller.injected.iter().filter(|t| t.name != super::PYTHON) {
        out.push(ToolDef {
            name: tool.name.clone(),
            description: tool.description.clone(),
            input_schema: tool.input_schema.clone(),
        });
    }
    Ok(out)
}

/// Write the module into this invocation's record directory, creating it
/// if the executor has not (a program raised by hand, ARCH §3.4).
fn write(record: &Path, module: &Path, source: &str) -> Result<(), Error> {
    std::fs::create_dir_all(record)
        .and_then(|()| std::fs::write(module, source))
        .map_err(|source| Error::Module {
            path: module.to_path_buf(),
            source,
        })
}

/// `PYTHONPATH` with the record directory first, so the generated module
/// wins over anything the operator's environment names, and whatever was
/// already there kept, so a deployment that ships its own libraries keeps
/// them.
fn path_with(env: &dyn EnvLookup, record: &Path) -> std::ffi::OsString {
    let mut path = record.as_os_str().to_owned();
    if let Some(existing) = env.get(PYTHONPATH) {
        path.push(":");
        path.push(existing);
    }
    path
}

#[cfg(test)]
mod tests;
#[cfg(test)]
mod tests_faults;
