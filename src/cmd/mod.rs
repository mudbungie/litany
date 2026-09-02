//! The command surface (ARCH §3.4 "One command surface, two bindings").
//!
//! This module is the one authoritative definition of what `litany` can
//! do: the [`Cli`]/[`Command`] clap surface, one entry per verb
//! ([`Command::run`] → `<verb>::run`), and the binding seam — [`Fx`] (the
//! injections a binding supplies), [`Outcome`] (a verb's product), and
//! [`Error`] (its uniform failure) — plus the [`prelude`] mechanisms a
//! binding performs before invoking a driver verb.
//!
//! **Two bindings, one surface (§3.4).** The library performs no
//! process-global or terminal effect: the running-binary path, the
//! `$EDITOR` spawn, the locked stdio, and the SIGTERM flag all arrive
//! through [`Fx`]; process-group leadership and stop-flag installation
//! are the [`prelude`] the binding runs. `src/bin/litany` is the exec
//! binding; an embedding consumer is the other. Both parse the *same*
//! [`Cli`] and drive the *same* [`Command::run`].
//!
//! **An agent id is validated where it enters.** Every verb that takes
//! an agent id from outside — `message`, `advance`, `stop`, `dispatch`,
//! `bundle`, `delete` — calls [`crate::name::require_agent_id`] before touching
//! disk, so an id that is not a single path component never reaches a
//! `join` (§2.3; see [`crate::name`] for why `Path::join` makes that
//! load-bearing). This surface is the only way in — both bindings enter
//! here, and a model's `message` / `dispatch` tool re-enters through it
//! (§3.4) — so one guard per verb covers every supplier.

/// The closed set of names this engine performs behind its own front door
/// (`litany tool <name>`, ARCH §3.3 third hop), sorted — the same const the
/// unknown-tool decline and `litany tool --help` render. On the surface
/// because a host installing a [`ToolInjection`] routes every invocation
/// itself, so it must ask which names the engine answers, not restate them.
pub use crate::prompt::tool::builtin::NAMES as BUILTIN_TOOLS;
pub use crate::prompt::tool::inject::{InjectedTool, RoutedCall, RoutedCapture, ToolInjection};
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;

pub mod advance;
pub mod bundle;
pub mod config;
pub mod delete;
pub mod dispatch;
pub mod message;
pub mod new;
pub mod prelude;
pub mod prime;
pub mod prompt;
pub mod replay;
pub mod retarget;
pub mod scan;
pub mod stop;
pub mod tool;
pub mod workflow;

#[cfg(test)]
mod tests;

/// The `litany` command-line surface (ARCH §3.4). Behaviourally identical
/// across both bindings — the argv shape here is the single source of
/// truth for the CLI, pinned by the `tests/*_cli.rs` end-to-end tests.
#[derive(clap::Parser, Debug)]
#[command(name = "litany", about = "Git-backed agent harness",
          version = crate::prompt::cli_version())]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,
}

/// Every verb, in a uniform shape: `Variant(<verb>::Args)`. The variant
/// doc comment is the subcommand's `--help` about text (§3.4); per-arg
/// help rides the `Args` fields.
#[derive(clap::Subcommand, Debug)]
pub enum Command {
    /// Create a new workspace (ARCH §2.2): a bare repo.git plus the
    /// first config commit on `config/default`. No argument creates
    /// `<data-root>/workspaces/<auto-id>/`; a path creates there.
    New(new::Args),
    /// Author a config commit beyond `litany new` (ARCH §2.2, §2.3): the
    /// only act that advances a config branch. Materializes a checkout of
    /// the target lineage, refreshes `descriptions/**` from the data-root
    /// pools (§3.3), opens it in `$EDITOR`, and commits. `<name>` defaults
    /// to `default`. `--from <source>` forks a new `config/<name>` off
    /// `config/<source>`; `--orphan` starts a fresh lineage.
    Config(config::Args),
    /// Send one user message on a fresh root branch; prints the new
    /// agent's **id** (§2.3) — not its `--name`, which is the optional
    /// display fact and never a product. The branch forks off
    /// `config/default`'s head by default; `--config <name>` starts on
    /// another config lineage, `--from <ref>` off any ref at all — a
    /// historical commit, a stopped tip (fork-from-history, ARCH §2.3,
    /// §7.2). The two are exclusive.
    Prompt(prompt::Args),
    /// Dispatch a subagent (ARCH §2.5, §3.4). `<role>` is `compactor`
    /// (§2.7) or `worker` (§2.5); future roles slot in by name. `--goal`
    /// is required for `worker`, rejected for `compactor` (§2.7).
    /// `--from <ref>` forks the child off that ref instead of the
    /// parent's tip (§2.3); its id, and so its return address, is
    /// unchanged.
    Dispatch(dispatch::Args),
    /// Move a running agent onto another config **lineage** (ARCH §2.2
    /// — resolution follows a lineage's tip by itself since bl-403b, so
    /// what retargets is the lineage, or the healing of a diverged
    /// one). Writes the mark `refs/litany/retarget/<agent>` at
    /// `config/<name>`'s head (`--config`, default `default`); the
    /// agent's own executor lands the re-fork at its next step. A
    /// target the agent already resolves is a clean no-op.
    Retarget(retarget::Args),
    /// Switch which workflow governs a running agent (ARCH §6 *The
    /// workflow mark*): writes the standing mark
    /// `refs/litany/workflow/<agent>` at `config/<name>`'s head
    /// (`--config`, default `default`), consulted at every step boundary
    /// — nearest mark on the agent's descent wins. `--clear` removes it,
    /// returning the agent to its followed config's workflow.
    Workflow(workflow::Args),
    /// Stop a conversation branch (ARCH §2.9 SIGTERM). Default stops the
    /// one agent; `--stop-children` also stops every descendant
    /// (`<branch>-*`, §2.3) — the opt-in agent→agent cascade.
    Stop(stop::Args),
    /// Deposit a message into an agent's inbox and probe the executor
    /// lock (ARCH §2.11, §3.4). Sender from `LITANY_CONV_BRANCH`. `agent`
    /// is the recipient id (== branch name / hyphenated descent).
    Message(message::Args),
    /// Operator verb: one workspace-wide silent-death sweep + inbox flush
    /// (ARCH §2.11, §8). Hand/cron only; never on a driver hot path.
    Scan(scan::Args),
    /// Archive an agent subtree (ARCH §9.2): git bundle of `<agent>` and
    /// its hyphen-descendants plus the `steps/`/`inbox/` slices, under
    /// `<out-dir>`.
    Bundle(bundle::Args),
    /// Remove an agent (ARCH §9.2 retention): its `agents/<id>` ref,
    /// worktree, `steps/`/`inbox/` slices and `refs/litany/*` marks.
    /// Declines a subtree the bare form did not ask for and a live
    /// driver's agent (§2.11); `--children` takes the whole subtree,
    /// `--dry-run` prints the same census and removes nothing. Archive
    /// first with `bundle` if you want it kept.
    Delete(delete::Args),
    /// Replay an archive (ARCH §9.2) into a scratch workspace under
    /// `LITANY_HOME`'s data root (`replays/<agent>/`); prints its path
    /// for the ordinary frontend (§3.5).
    Replay(replay::Args),
    /// Drive one agent's branch forward (ARCH §6): take the lease (adopt
    /// LITANY_LOCK_FD or acquire), deliver pending mail, run the next
    /// step, and exec the successor hop. The target every launch seam
    /// spawns; also an operator verb.
    Advance(advance::Args),
    /// In-process built-in tool entry (ARCH §3.3): `tool_use.input` JSON
    /// on stdin, bytes on stdout, exit 0/non-zero. Third resolver hop
    /// (`<data-root>/tools/litany-tool-<name>` → PATH → `<litany> tool …`).
    Tool(tool::Args),
    /// Found the installation substrate (ARCH §2.2): resolve the harness
    /// root and seed the default `models.yaml`, the pools, and the
    /// `workflows/`/`workspaces/` dirs — seed-if-absent. `make install` runs it.
    Prime(prime::Args),
}

/// A verb's one product (ARCH §3.4 one-product convention). The binding
/// performs it: [`Line`](Outcome::Line) is the verb's single stdout
/// product (new → dest path, prompt → branch, scan → report, replay →
/// scratch path); [`Quiet`](Outcome::Quiet) is a product-less success;
/// [`Exec`](Outcome::Exec) is the §6 advance successor handoff, which the
/// exec binding `execve`s; [`Code`](Outcome::Code) is the `tool` verb's
/// process exit status (§3.3 is_error contract).
#[derive(Debug)]
pub enum Outcome {
    /// The verb's single stdout line.
    Line(String),
    /// Product-less success — nothing printed.
    Quiet,
    /// The advance successor command to `exec` (§6 exec baton). An
    /// `AdvanceHandoff::Done` hop maps to [`Quiet`](Outcome::Quiet).
    Exec(std::process::Command),
    /// The `tool` verb's desired process exit code (§3.3).
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
    /// (`<driver_target> tool <name>`), and the `dispatch` / `message`
    /// built-ins' own re-entry. ARCH §2.11: "the driver target is
    /// injected at the binding, not resolved by name" — the exec binding
    /// resolves it once via `std::env::current_exe`, a linked host names
    /// its own re-exec target or a PATH-resolved `litany`. The library
    /// resolves none of its own.
    pub driver_target: PathBuf,
    /// The provider-adapter target (ARCH §4.4), injected the same way as
    /// [`Self::driver_target`]: the library resolves no binary of its own,
    /// the binding names it. `None` — the exec binding's default — leaves
    /// today's resolution intact (the `models.yaml` `adapter:` override,
    /// else `bz` on PATH, §4.2). An embedding host that re-execs *itself*
    /// as the adapter names its own target here; like an explicit override,
    /// a named target skips the load-time version guard and the in-band
    /// `MessageStart.v` handshake governs (§4.4).
    pub adapter_target: Option<PathBuf>,
    /// The `litany config` `$EDITOR` hand-off (§2.2) — the interactive
    /// spawn the exec binding supplies as `cli::edit_in_editor`.
    pub editor: &'a dyn Fn(&Path) -> std::io::Result<()>,
    /// The `litany tool` stdin (§3.3 `tool_use.input` JSON).
    pub tool_stdin: &'a mut dyn std::io::Read,
    /// The `litany tool` stdout (§3.3 raw result bytes).
    pub tool_stdout: &'a mut dyn std::io::Write,
    /// The `litany tool` stderr (§3.3 stderr-concat contract).
    pub tool_stderr: &'a mut dyn std::io::Write,
    /// The executor's SIGTERM flag (§2.9 step 3), the driver verbs'
    /// `Deps::stop`. The exec binding wires [`prelude::stop_flag`] after
    /// [`prelude::install_stop_handler`].
    pub stop: &'a AtomicBool,
    /// The binding's **tool injection** (ARCH §3.3 *Host-injected
    /// tools*), injected like [`Self::driver_target`], and its one choice
    /// of execution pipeline: `None` (the exec binding) spawns every tool
    /// through the §3.3 three hops; a host supplying one has its
    /// [`ToolInjection::tools`] declared *and* permitted on every request
    /// and its [`ToolInjection::route`] answering *every* invocation, with
    /// no resolution behind it. [`ToolInjection`], DESIGN_TOOL_INJECTION §3.4.
    pub tool_injection: Option<&'a dyn ToolInjection>,
}

/// A verb's uniform failure. `Display` renders exactly today's stderr
/// shape `litany <verb-prefix>: <error>` (dispatch's prefix is `dispatch
/// <role>`; tool's is `tool <name>`), which the binding prints before a
/// non-zero exit.
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

impl Command {
    /// The §2.9 preludes this verb needs, in the order a binding must
    /// perform them (ARCH §3.4 binding-preludes seam). A driver verb
    /// that owns a step loop takes a process group so the §2.9 cascade
    /// reaches its own adapter and tool subprocesses, and installs the
    /// stopped-deposit handler; `dispatch` (child re-entry) takes a
    /// group but drives nothing, so it installs no handler; every other
    /// verb needs neither.
    ///
    /// The map lives here, not in a binding, so both bindings read one
    /// tested fact rather than each keeping a match in step with it. The
    /// library only *names* the mechanisms — invoking them is the
    /// binding's act (§3.4 "Process effects stay at the binding").
    pub fn preludes(&self) -> &'static [fn()] {
        match self {
            Command::Prompt(_) | Command::Advance(_) => {
                &[prelude::become_pgid_leader, prelude::install_stop_handler]
            }
            Command::Dispatch(_) => &[prelude::become_pgid_leader],
            _ => &[],
        }
    }

    /// Run the parsed verb against the binding's [`Fx`] (ARCH §3.4). One
    /// arm per verb, each delegating to its module's `run`; the verb owns
    /// its [`Error`] prefix and its [`Outcome`].
    pub fn run(self, fx: &mut Fx) -> Result<Outcome, Error> {
        match self {
            Command::New(a) => new::run(a, fx),
            Command::Config(a) => config::run(a, fx),
            Command::Prompt(a) => prompt::run(a, fx),
            Command::Dispatch(a) => dispatch::run(a, fx),
            Command::Retarget(a) => retarget::run(a, fx),
            Command::Workflow(a) => workflow::run(a, fx),
            Command::Stop(a) => stop::run(a, fx),
            Command::Message(a) => message::run(a, fx),
            Command::Scan(a) => scan::run(a, fx),
            Command::Bundle(a) => bundle::run(a, fx),
            Command::Delete(a) => delete::run(a, fx),
            Command::Replay(a) => replay::run(a, fx),
            Command::Advance(a) => advance::run(a, fx),
            Command::Tool(a) => tool::run(a, fx),
            Command::Prime(a) => prime::run(a, fx),
        }
    }
}
