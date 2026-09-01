//! Harness-side of the provider-adapter contract (ARCH §4.4).
//!
//! The provider adapter is brazen's `bz` — one stateless binary for
//! every provider. The harness invokes it **once per attempt** as
//! `bz --json --provider <row>`, pipes a canonical request (JSON) to its
//! stdin, and reads its stdout as brazen's `v=1` canonical event stream
//! (NDJSON — one event per line). [`AdapterRunner`] is the exec seam the
//! retry driver ([`super::dispatch`]) depends on; [`SpawnAdapter`] is
//! the production implementation, tests inject a stub.
//!
//! Per-line dispatch is structural: the §4.4 stream emits one event per
//! line, and the harness is the live writer of
//! `<conv-repo>/steps/<conv-id>/<NNN>/response.json` (§3.5). Routing
//! lines through a callback lets the harness append each event to disk
//! as it arrives and stream *content* into the transcript writer's
//! staging sink (§2.3) in the same pass — one stream, two sinks, no
//! read-back.
//!
//! **Exit code is diagnostic (§4.4).** brazen surfaces every failure
//! in-band as an `Error` event on stdout and *also* sets a sysexits
//! exit code computed from the same fact — the event is authoritative,
//! the exit code diagnostic. So a non-zero `bz` exit is NOT a spawn
//! error here: only a failure to *launch* the binary is. brazen dies at
//! once on SIGTERM with no flush (§2.9); the missing trailing `end` on
//! the closed fd is the stop signature, handled by classification, not
//! this runner.
//!
//! **Stderr is the adapter's diagnostic channel**, and the run's
//! product beside the stdout stream. An adapter that dies *before* it
//! can speak the in-band contract — a malformed brazen config, an
//! unreadable credstore — says so only there, so discarding it turns a
//! startup failure into an empty stdout stream indistinguishable from a
//! mid-stream kill (§2.9). The runner captures it whole and hands it
//! back; the caller lands it in the step record and quotes its tail
//! when the stream ends without a terminal `end` (§2.3, §4.4). It is
//! read concurrently with stdout on its own thread, so a chatty adapter
//! filling the stderr pipe buffer can never deadlock against the
//! harness tailing stdout.
//!
//! **No env forwarding.** Auth and endpoints are entirely brazen's
//! (§4.4): its config resolves via `--config` / `BRAZEN_CONFIG` / XDG,
//! and the harness sets `BRAZEN_CONFIG` only under test isolation — as
//! an inherited process env, never a per-model-call value the harness
//! threads. The child inherits the harness environment unchanged.

use super::Error;
use std::ffi::OsString;
use std::io::{self, BufRead, BufReader, Read, Write};
use std::path::Path;
use std::process::{Command, Stdio};
use std::thread;

/// The provider-adapter binary: brazen's `bz`, resolved on `PATH`
/// unless the global `models.yaml` names an `adapter:` override (ARCH
/// §4.2 / §4.4).
pub const BZ_BIN: &str = "bz";

/// Resolve the adapter binary. One resolution order, most-specific
/// first: the `models.yaml` `adapter:` override (§4.2), else the
/// binding-injected `host` target (`cmd::Fx::adapter_target` — an
/// embedding host naming itself as the adapter, the same injection
/// philosophy as `driver_target`, §3.4), else `bz` on `PATH`. Both named
/// targets are used verbatim, and both skip the load-time version guard
/// in favor of the in-band `MessageStart.v` handshake (§4.4): a named
/// target — config override or host assertion — is identity the caller
/// vouches for, one trust class. The version guard runs only for the
/// default `PATH`-resolved `bz`, when both are `None`.
pub fn resolve_binary(adapter_override: Option<&Path>, host: Option<&Path>) -> OsString {
    match adapter_override.or(host) {
        Some(path) => path.as_os_str().to_os_string(),
        None => OsString::from(BZ_BIN),
    }
}

/// The slice of the adapter contract the harness calls into.
///
/// One subprocess per [`Self::run`]. As the child writes stdout, every
/// completed line (terminator stripped, blanks skipped) is handed to
/// `on_line`. The callback may surface an [`io::Error`] to abort early;
/// otherwise the call returns when the child exits and stdout reaches
/// EOF. A non-zero exit is NOT surfaced — the in-band `Error` event is
/// authoritative and the exit code is diagnostic (§4.4). Only a failure
/// to spawn the binary surfaces as an error.
pub trait AdapterRunner {
    /// Spawn `binary` with `args`, write `stdin_bytes` to its stdin
    /// (closing it after), and route each stdout line through `on_line`.
    ///
    /// Returns the child's **stderr, captured whole** — the adapter's
    /// diagnostic channel, empty on an ordinary run. It is a return
    /// value rather than a second callback because its one consumer
    /// wants it entire: the step record's `stderr.log` and the tail
    /// quoted in a half-stream error (§2.3, §4.4).
    fn run(
        &self,
        binary: &OsString,
        args: &[&str],
        stdin_bytes: &[u8],
        on_line: &mut dyn FnMut(&[u8]) -> io::Result<()>,
    ) -> io::Result<Vec<u8>>;
}

/// Default [`AdapterRunner`]. Uses [`Command`] with PATH lookup and
/// inherits the harness environment (test isolation sets `BRAZEN_CONFIG`
/// there, §4.4).
#[derive(Debug, Clone, Copy)]
pub struct SpawnAdapter;

impl AdapterRunner for SpawnAdapter {
    fn run(
        &self,
        binary: &OsString,
        args: &[&str],
        stdin_bytes: &[u8],
        on_line: &mut dyn FnMut(&[u8]) -> io::Result<()>,
    ) -> io::Result<Vec<u8>> {
        let mut child = Command::new(binary)
            .args(args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?;
        // Stdin in a thread so a slow / large request never deadlocks
        // against the child's stdout pipe-buffer fill (the harness
        // tails stdout on the main thread). Writes are best-effort: if
        // `bz` errors before reading stdin, the broken pipe is not a
        // fault — its `Error` event is already on stdout (§4.4).
        let mut stdin = child.stdin.take().expect("stdin is piped");
        let stdin_owned = stdin_bytes.to_vec();
        let stdin_thread = thread::spawn(move || {
            let _ = stdin.write_all(&stdin_owned);
            // Drop closes the fd, signaling EOF.
        });

        // Stderr on its own thread for the same reason: the harness is
        // busy tailing stdout, and a full stderr pipe buffer would
        // otherwise block the child mid-stream.
        let mut stderr = child.stderr.take().expect("stderr is piped");
        let stderr_thread = thread::spawn(move || {
            let mut captured = Vec::new();
            let _ = stderr.read_to_end(&mut captured);
            captured
        });

        let stdout = child.stdout.take().expect("stdout is piped");
        let mut reader = BufReader::new(stdout);
        let mut buf = Vec::new();
        loop {
            buf.clear();
            let n = reader.read_until(b'\n', &mut buf)?;
            if n == 0 {
                break;
            }
            let line = strip_trailing_lf(&buf);
            if line.is_empty() {
                continue;
            }
            on_line(line)?;
        }

        stdin_thread.join().expect("stdin writer thread panicked");
        let captured = stderr_thread.join().expect("stderr reader thread panicked");
        // Exit status is diagnostic only (§4.4) — never surfaced.
        let _ = child.wait()?;
        Ok(captured)
    }
}

/// Run `binary` with `args`, discarding stdin, and return its stdout as
/// one UTF-8 string (lines rejoined by `\n`). Used by the load-time
/// version guard (`bz --version`, §4.4) — the single stdout line `bz`
/// prints is captured through the same exec seam so the guard is
/// stub-testable.
pub fn capture_stdout(
    runner: &dyn AdapterRunner,
    binary: &OsString,
    args: &[&str],
) -> io::Result<String> {
    let mut out: Vec<u8> = Vec::new();
    // The guard reads stdout only; a `--version` probe's stderr has no
    // step record to land in and nothing to say (§4.4).
    runner.run(binary, args, b"", &mut |line| {
        if !out.is_empty() {
            out.push(b'\n');
        }
        out.extend_from_slice(line);
        Ok(())
    })?;
    Ok(String::from_utf8_lossy(&out).into_owned())
}

/// Load-time version guard (§4.4): `bz --version` must report the exact
/// version of the linked brazen crate ([`super::brazen_pin`]); a
/// mismatch is declined (PRINCIPLES "Decline illegal operations")
/// rather than silently downgraded. Lives with the adapter machinery it
/// drives ([`capture_stdout`], [`spawn_error`]); its one caller is
/// resolution (`super::resolve`), which skips it for a named target
/// (§4.4).
pub(super) fn check_bz_version(
    adapter: &dyn AdapterRunner,
    binary: &OsString,
) -> Result<(), Error> {
    let out =
        capture_stdout(adapter, binary, &["--version"]).map_err(|e| spawn_error(binary, e))?;
    // `bz --version` prints e.g. `bz 0.0.2`; the version is the last
    // whitespace token.
    let found = out.split_whitespace().last().unwrap_or("").to_string();
    if found != super::brazen_pin() {
        return Err(Error::VersionSkew {
            found,
            expected: super::brazen_pin().to_string(),
        });
    }
    Ok(())
}

/// Classify a failure to *launch* the adapter (§4.4) — the one
/// classification the harness makes over a spawn `io::Error`, and the
/// reason both spawn seams (the version guard's `--version` probe and
/// every model call) route through here rather than mapping the errno
/// straight onto [`Error::AdapterSpawn`]. `NotFound` means the binary
/// is simply not there, which is actionable, so it earns the version
/// guard's voice ([`Error::AdapterMissing`]); everything else is a real
/// spawn failure with nothing to advise.
pub(super) fn spawn_error(binary: &OsString, source: io::Error) -> Error {
    match source.kind() {
        io::ErrorKind::NotFound => Error::AdapterMissing {
            binary: binary.to_string_lossy().into_owned(),
            pin: super::brazen_pin().to_string(),
            source,
        },
        _ => Error::AdapterSpawn(source),
    }
}

/// Strip a single trailing `\n` (and the `\r` of a `\r\n` pair) from
/// `buf` so callbacks see clean payload bytes.
fn strip_trailing_lf(buf: &[u8]) -> &[u8] {
    let trimmed = buf.strip_suffix(b"\n").unwrap_or(buf);
    trimmed.strip_suffix(b"\r").unwrap_or(trimmed)
}

#[cfg(test)]
mod tests;
