//! **The binding seam** (ARCH §3.4 "Process effects stay at the
//! binding"): the three types a binding speaks to a verb in — the
//! injections it supplies ([`Fx`]), the one product it performs
//! ([`Outcome`]) and the uniform failure it prints ([`Error`]).
//!
//! Split from [`super`], which keeps the verb set — the [`Cli`](super::Cli)
//! surface, the [`Command`](super::Command) enum and its one entry per
//! verb. Two different things live at that boundary and only one of them
//! grows with the verb list; a module that held both grew past the
//! repo's per-file cap on the verb that arrived next
//! (`docs/DESIGN_LEARNING_LOOP.md` §3's `litany proposal`, bl-9a65).
//! The three types stay re-exported at `cmd::*`, which is the path every
//! consumer and the parity checker's seam ledger name.

use super::ToolInjection;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;

/// A verb's one product (ARCH §3.4 one-product convention). The binding
/// performs it: [`Line`](Outcome::Line) is the verb's single stdout
/// product; [`Quiet`](Outcome::Quiet) is a product-less success;
/// [`Exec`](Outcome::Exec) is the §6 advance successor handoff the exec
/// binding `execve`s; [`Code`](Outcome::Code) is the exit status the
/// `tool` and `invoke` verbs end on (§3.3 is_error contract).
#[derive(Debug)]
pub enum Outcome {
    /// The verb's single stdout line.
    Line(String),
    /// Product-less success — nothing printed.
    Quiet,
    /// The advance successor command to `exec` (§6 exec baton). An
    /// `AdvanceHandoff::Done` hop maps to [`Quiet`](Outcome::Quiet).
    Exec(std::process::Command),
    /// The `tool` / `invoke` verbs' desired exit code (§3.3).
    Code(u8),
}

/// The binding's injections (ARCH §3.4 "Process effects stay at the
/// binding"). Every process-global or terminal effect a verb needs is a
/// field here, supplied by the binding — the library reaches for none of
/// its own.
pub struct Fx<'a> {
    /// The re-entry path for **every** seam that goes back through the
    /// front door: the detached `litany advance` launch and the §6
    /// successor `execve` (§2.11), the §3.3 tool resolver's third hop
    /// (`<driver_target> tool <name>`), the door verb's own executor,
    /// and the `dispatch` / `message` built-ins' re-entry. ARCH §2.11
    /// holds the rule — injected at the binding, never resolved by
    /// name — and the exec binding is the only `current_exe` reader.
    pub driver_target: PathBuf,
    /// The provider-adapter target (ARCH §4.4), injected the same way
    /// as [`Self::driver_target`]. `None` — the exec binding's default
    /// — leaves §4.2's resolution intact (`models.yaml`'s `adapter:`,
    /// else `bz` on PATH); a named target skips the load-time version
    /// guard, the in-band `MessageStart.v` handshake governing (§4.4).
    pub adapter_target: Option<PathBuf>,
    /// The `litany config` `$EDITOR` hand-off (§2.2) — the interactive
    /// spawn the exec binding supplies as `cli::edit_in_editor`.
    pub editor: &'a dyn Fn(&Path) -> std::io::Result<()>,
    /// The `tool` / `invoke` stdin (§3.3: `tool_use.input`, a block).
    pub tool_stdin: &'a mut dyn std::io::Read,
    /// Their stdout (§3.3 raw result bytes).
    pub tool_stdout: &'a mut dyn std::io::Write,
    /// Their stderr (§3.3 stderr-concat contract).
    pub tool_stderr: &'a mut dyn std::io::Write,
    /// The executor's SIGTERM flag (§2.9 step 3), the driver verbs'
    /// `Deps::stop` — [`prelude::stop_flag`], once the handler is in.
    pub stop: &'a AtomicBool,
    /// The binding's **tool injection** (ARCH §3.3 *Host-injected
    /// tools*), injected like [`Self::driver_target`], and its one
    /// choice of execution pipeline: `None` (the exec binding) spawns
    /// every tool through the §3.3 three hops; a host supplying one has
    /// its tools declared *and* permitted and its router answering
    /// *every* invocation ([`ToolInjection`], DESIGN_TOOL_INJECTION §3.4).
    pub tool_injection: Option<&'a dyn ToolInjection>,
}

/// A verb's uniform failure. `Display` renders the stderr shape
/// `litany <verb-prefix>: <error>` (dispatch's is `dispatch <role>`,
/// tool's `tool <name>`), which the binding prints before a non-zero exit.
#[derive(Debug)]
pub struct Error {
    prefix: String,
    message: String,
}

impl Error {
    /// Build a failure carrying `prefix` (the verb prefix, without the
    /// leading `litany `) and the `Display` of the underlying error.
    pub fn new(prefix: impl Into<String>, source: impl std::fmt::Display) -> Self {
        Self {
            prefix: prefix.into(),
            message: source.to_string(),
        }
    }
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "litany {}: {}", self.prefix, self.message)
    }
}

impl std::error::Error for Error {}
