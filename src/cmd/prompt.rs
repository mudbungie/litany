//! `litany prompt` — drive one root conversation (ARCH §2.3). The §2.9
//! preludes (`become_pgid_leader` + `install_stop_handler`) are the
//! binding's, run before [`run`] (ARCH §3.4 binding-preludes seam,
//! [`super::prelude`]); this entry only builds the deps and drives.

use super::{Error, Fx, Outcome};
use crate::harness_root;
use crate::prompt::inbox::AdvanceLauncher;
use crate::prompt::{self, NanoIdGen, SpawnAdapter, SpawnTool, SystemClock};
use crate::template::RealGit;
use crate::workspace;
use std::path::PathBuf;

/// `litany prompt <repo> <message> [--from <ref>] [--config <name>]
/// [--name <name>] [--pin <dest>=<src>]... [--cwd <path>]`.
#[derive(clap::Args, Debug)]
pub struct Args {
    /// Path to the workspace (conversation repo) root.
    pub repo: PathBuf,
    /// Opening message for the new root conversation.
    pub message: String,
    /// Fork off this ref instead of a config lineage's head (ARCH §2.3,
    /// §7.2): any commit of any agent, a stopped tip, a config commit.
    #[arg(long)]
    pub from: Option<String>,
    /// Fork off `config/<name>`'s head instead of `config/default` (ARCH
    /// §2.2). Mutually exclusive with `--from`.
    #[arg(long)]
    pub config: Option<String>,
    /// Display name for the new agent (ARCH §2.3): one unbroken word,
    /// unique among the workspace's living agents, set here and never
    /// rewritten. `litany message` accepts it in place of the agent id.
    /// Omitted, it is minted as two PascalCase words (`PeachHollow`).
    #[arg(long)]
    pub name: Option<String>,
    /// Pin a caller-supplied document (ARCH §2.5): freeze `<src>`'s
    /// exact bytes at worktree-relative `<dest>` on the dispatch commit,
    /// beside `goal.md` and `soul.md`. Repeatable; validated — and
    /// refused — before any branch or ref exists
    /// ([`crate::prompt::pinned_doc`]).
    #[arg(long = "pin", value_name = "DEST=SRC")]
    pub pin: Vec<String>,
    /// Start the agent working in this directory instead of its worktree
    /// (ARCH §3.3): seeds the working-directory mark the `cd` built-in
    /// otherwise writes, before the first step. Validated — and refused —
    /// before any branch or ref exists.
    #[arg(long, value_name = "PATH")]
    pub cwd: Option<PathBuf>,
}

/// Spawn the root agent branch and drive its step loop; print the agent
/// id (§2.3) — the verb's one product. The `String`→[`Outcome::Line`]
/// map and the failure conversion are fn-pointers, so the success arm
/// carries no test-only region (its happy path needs a live provider,
/// pinned by `tests/prompt_end_to_end.rs`).
pub fn run(args: Args, fx: &mut Fx) -> Result<Outcome, Error> {
    go(args, fx)
        .map(Outcome::Line)
        .map_err(|e| Error::new("prompt", e))
}

/// No workspace scan: drivers touch only their own branch (§2.11). The
/// detached-launch and successor target is
/// [`Fx::driver_target`](super::Fx::driver_target); the stop flag is
/// [`Fx::stop`](super::Fx::stop).
fn go(args: Args, fx: &mut Fx) -> Result<String, Box<dyn std::error::Error>> {
    // Pins are validated and their sources read here, first — every
    // refusal precedes the fork, so no branch, ref or inference exists
    // when one fires (ARCH §2.5, [`crate::prompt::pinned_doc`]). `--cwd`
    // joins them under the same rule, through the mark's own validation
    // (ARCH §3.3, [`crate::workspace::cwd::resolve`] — the `cd`
    // built-in's rules, applied earlier).
    let pins = prompt::pinned_doc::load(&args.pin)?;
    let cwd = args
        .cwd
        .as_deref()
        .map(workspace::cwd::resolve)
        .transpose()?;
    let roots = harness_root::resolve()?;
    // The binding-injected driver target (§2.11 "injected at the binding,
    // not resolved by name") serves both re-entry seams: the §3.3 tool
    // resolver's third hop and the §2.11/§6 detached launch. No
    // `current_exe` here — under a linked host it would name the host.
    let tool_executor = SpawnTool::new(&roots.data, &SystemClock, &fx.driver_target)
        .with_injection(fx.tool_injection);
    let launcher = AdvanceLauncher::with_exe(fx.driver_target.clone());
    let deps = prompt::Deps {
        adapter: &SpawnAdapter,
        sleeper: &prompt::RealSleeper,
        git: &RealGit::new(),
        clock: &SystemClock,
        id_gen: &NanoIdGen,
        tool_executor: &tool_executor,
        config_root: &roots.config,
        adapter_target: fx.adapter_target.as_deref(),
        stop: fx.stop,
        launcher: &launcher,
        rng: &crate::mint::SplitMix64::from_entropy(),
    };
    prompt::run(
        &args.repo,
        &args.message,
        args.from.as_deref(),
        args.config.as_deref(),
        args.name.as_deref(),
        &pins,
        cwd.as_deref(),
        &deps,
    )
    .map_err(Into::into)
}
