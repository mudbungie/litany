//! `litany prompt` — root-conversation backend (ARCH §2.3).
//!
//! Each prompt spawns an `agents/<conv-id>` branch off the ref the
//! start names ([`fork_point`] — a config lineage's head by default,
//! any ref with `--from`; §2.2–§2.3, there is no `main`), commits the
//! dispatch commit (§2.10) — which also removes the harness-facing
//! control files from the agent's tree (§2.2) and derives its
//! `descriptions/**` from the governing config commit (§3.3) — drives
//! the step loop through brazen's `bz` (§4.4), and lands each step's
//! response as attempt segments. Merge-back is gone: the root branch
//! persists on its own ref (§2.4), and a child returns by depositing a
//! result message at the address its epitaph names (§2.6).
//!
//! Provider plumbing follows ARCH §4.4: every model call execs `bz`
//! (`bz --json --provider <row>`, canonical request on stdin, `v=1`
//! events on stdout) once per attempt, with the harness owning the retry
//! loop (§2.10). Auth and endpoints are entirely brazen's; the harness
//! references a provider *row* by name and never sees credential
//! material (§4.1). Config: the global `<harness-root>/models.yaml`
//! carries only the optional `adapter:` override (§4.2, bl-35e2); the
//! config commit's `providers.yaml` carries the whole
//! role → (provider row, model, tools) mapping (§4.3), read from the
//! governing config commit (§2.2). Retry policy (attempt cap + backoff)
//! is `workflow.yaml`'s (§6).
//!
//! [`run`] is orchestrated against injected [`AdapterRunner`],
//! [`Sleeper`], [`GitRunner`], [`Clock`], and [`IdGen`] so every branch
//! of the flow is exercisable without a live provider or on-disk side
//! effects.

pub mod adapter;
pub mod budget;
pub mod child_dispatch;
pub mod clock;
pub mod compactor;
pub mod dispatch;
pub mod dispatch_cli;
mod error;
pub mod fork_point;
pub mod inbox;
pub(crate) mod notice;
mod pin;
pub mod pinned_doc;
pub(crate) mod rebase_forward;
mod resolve;
pub mod retarget;
pub mod role;
pub mod step;
pub mod stop;
pub mod subagent;
pub mod tool;
mod workflow_actions;

#[cfg(test)]
mod tests;

pub use adapter::{AdapterRunner, SpawnAdapter};
pub use child_dispatch::ChildDispatchRequest;
pub use clock::{Clock, IdGen, NanoIdGen, SystemClock};
pub use dispatch::{RealSleeper, Sleeper, install_stop_handler, stop_flag};
pub use error::Error;
pub use pin::{brazen_pin, cli_version};
pub use pinned_doc::{PinnedDoc, PinnedDocs};
pub use tool::{ExecError, SpawnTool, ToolExecutor};

use crate::template::GitRunner;
use crate::workspace::agent_name as name_fact;
use std::path::Path;

/// Role name resolved from the config commit's `providers.yaml`
/// (`roles:` block, ARCH §4.3) to drive the root conversation.
pub(crate) const WORKER_ROLE: &str = "worker";
/// Directory in the config commit's tree holding the role souls (ARCH
/// §4.3 — soul = `souls/<role>.md` in the governing config commit).
pub(crate) const SOULS_DIR: &str = "souls";
/// Control file naming the role → (provider row, model, tools)
/// assignments (ARCH §4.3). Read from the governing config commit's
/// tree (§2.2), never from a worktree file.
const PER_REPO_PROVIDERS_FILE: &str = "providers.yaml";
/// Control file carrying the §6 event bindings, retry policy and
/// budgets. Read from the governing config commit's tree (§2.2) by both
/// the worker resolution ([`resolve`]) and the dispatch budget gate
/// ([`child_dispatch`]) — one name, one home.
pub(crate) const WORKFLOW_FILE: &str = "workflow.yaml";
/// Global control file naming the optional `adapter:` override (ARCH
/// §4.2, bl-35e2 — no models table). Lives at the harness root.
const GLOBAL_MODELS_FILE: &str = "models.yaml";

/// Dependencies [`run`] orchestrates over. Held as `&dyn` so the
/// struct itself carries no generic parameters and tests can pass
/// stubs inline. `config_root` is the config-lifetime harness root
/// (ARCH §2.2), which holds the global `models.yaml` (ARCH §4.2);
/// production passes [`crate::harness_root::Roots::config`], tests pass
/// a temp dir. The data-lifetime root reaches [`run`] only through
/// `tool_executor`, which already carries it.
pub struct Deps<'a> {
    pub adapter: &'a dyn AdapterRunner,
    pub sleeper: &'a dyn Sleeper,
    pub git: &'a dyn GitRunner,
    pub clock: &'a dyn Clock,
    pub id_gen: &'a dyn IdGen,
    pub tool_executor: &'a dyn ToolExecutor,
    pub config_root: &'a Path,
    /// The binding-injected adapter target (`cmd::Fx::adapter_target`,
    /// ARCH §3.4): an embedding host naming itself (or another binary) as
    /// the provider adapter, the same injection philosophy as
    /// `driver_target` — the library resolves no binary of its own. `None`
    /// (the exec binding's default) leaves today's resolution intact: the
    /// `models.yaml` `adapter:` override, else `bz` on PATH (§4.2). When
    /// set it sits below an explicit override in the one resolution order
    /// and, like an override, skips the load-time version guard (§4.4).
    pub adapter_target: Option<&'a Path>,
    /// The executor's SIGTERM flag (ARCH §2.9 step 3), observed at the
    /// step-loop check points. Production wires the process-wide
    /// [`dispatch::stop_flag`] after [`dispatch::install_stop_handler`];
    /// tests inject a constructed [`std::sync::atomic::AtomicBool`] so the
    /// stopped-deposit path is exercised without a real signal.
    pub stop: &'a std::sync::atomic::AtomicBool,
    /// The driver launcher for the §2.11 exit protocol's self-directed
    /// launch, fired after the executor releases its lock on a
    /// final-response exit. Production wires [`inbox::AdvanceLauncher`]
    /// (the detached `litany advance` spawn, §2.11/§6); tests inject
    /// a recording launcher so the launch decision and its ordering are
    /// observable.
    pub launcher: &'a dyn inbox::Launcher,
    /// The randomness the agent-name mint draws on when a creation path
    /// omits `name` (ARCH §2.3, yog bl-aca4 — the settle-the-name
    /// pre-flight). Production wires
    /// [`name_fact::mint::SplitMix64::from_entropy`]; tests inject a
    /// seeded or scripted generator so the minted name is deterministic.
    pub rng: &'a dyn name_fact::mint::Rng,
}

/// **Seed a fresh agent's working directory** (ARCH §3.3) — the one
/// home for the act, called by both creation paths at the same moment:
/// after the agent's id has settled, before anything of the agent
/// exists. `dir` is already resolved and refused-if-illegal at the
/// binding ([`crate::workspace::cwd::resolve`]), so the only failure
/// left here is git's, and it fails the creation rather than starting an
/// agent in a directory its caller did not ask for.
///
/// `None` is the ordinary case — the mark stays unset and the agent
/// works in its worktree, which is the general path with the fact
/// absent, not a bootstrap case. **Nothing inherits a mark**: it is
/// keyed by agent id, no fork, merge or transfer moves one, and this
/// function never reads the dispatcher's. A child is in its own worktree
/// unless its own creation named a directory.
pub(crate) fn seed_cwd(
    repo: &Path,
    agent_id: &str,
    dir: Option<&Path>,
    git: &dyn crate::template::GitRunner,
) -> Result<(), Error> {
    match dir {
        None => Ok(()),
        Some(dir) => {
            crate::workspace::cwd::write(repo, agent_id, dir, git).map_err(|source| Error::Git {
                op: "seed working directory",
                source,
            })
        }
    }
}

/// Drive one root conversation against the workspace at `repo`: check
/// the layout (§2.2 — pre-v1 clean break on the retired
/// per-conversation layout), resolve the fork point the start names
/// (`--from` / `--config`, [`fork_point`], §2.3), pre-flight the
/// display name (§2.3), resolve the worker role against that fork
/// point's governing config commit, run the load-time version guard,
/// spawn the agent branch off it, and drive the step loop through `bz`.
/// Returns the agent id (the full hyphenated descent — the branch ref is
/// `agents/<id>`, ARCH §2.3). `pins` are the caller-supplied pinned
/// documents ([`pinned_doc`], §2.5) the dispatch commit snapshots beside
/// `goal.md` and `soul.md`; a pin-less start passes
/// [`PinnedDocs::none`]. `cwd` is the caller-seeded working directory
/// (§3.3, `litany prompt --cwd`), already resolved by
/// [`crate::workspace::cwd::resolve`]; `None` leaves the mark unset and
/// the agent works in its worktree.
#[allow(clippy::too_many_arguments)]
pub fn run(
    repo: &Path,
    msg: &str,
    from: Option<&str>,
    config: Option<&str>,
    name: Option<&str>,
    pins: &PinnedDocs,
    cwd: Option<&Path>,
    deps: &Deps<'_>,
) -> Result<String, Error> {
    crate::workspace::require(repo)?;
    let fork_point = fork_point::resolve(repo, from, config, deps.git)?;
    // Settle the name (§2.3, yog bl-aca4): supplied → validated against
    // the living agents; absent → minted against the same scan. No root
    // starts nameless.
    let name = name_fact::mint::preflight(repo, name, deps.git, deps.rng)?;
    let cfg = resolve::resolve_worker(repo, resolve::ConfigSource::Fork(&fork_point), deps)?;
    dispatch::run_exchange(
        repo,
        msg,
        &fork_point,
        Some(&name),
        pins,
        cwd,
        &cfg.as_resolved(),
        deps,
    )
}
