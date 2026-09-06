//! What a built-in derives about **its own caller** from the §3.3
//! harness contract: the installation roots, and the config commit the
//! calling agent follows.
//!
//! Two built-ins ask both questions — `load_skill`, which resolves a
//! body over the lineage's `skills/` and the install pool, and
//! `remember`, which stages a `facts.md` patch against the lineage
//! (`docs/DESIGN_CONTEXT_ECONOMY.md` §3). They were one home and a copy
//! until the second arrived; a fact answered twice is a fact that
//! drifts, so both answers live here and each caller maps the failure
//! into its own error voice.
//!
//! Nothing here reads the process environment. The tool is spawned with
//! the contract on it (§3.3), and the lookup arrives as [`EnvLookup`],
//! so a beat scripts the environment its subject sees instead of the
//! one the test binary happens to be running under.

use super::dispatch::EnvLookup;
use crate::harness_root::{self, Roots};
use crate::template::GitRunner;
use crate::workspace;
use std::io;
use std::path::Path;

/// Env keys the root resolution reads — the same three
/// [`harness_root::resolve`] reads for itself (ARCH §2.2).
pub(super) const ENV_LITANY_HOME: &str = "LITANY_HOME";
pub(super) const ENV_XDG_DATA: &str = "XDG_DATA_HOME";
pub(super) const ENV_HOME: &str = "HOME";

/// The installation roots as this tool's caller resolves them — the XDG
/// split, collapsed by `LITANY_HOME` (ARCH §2.2). `XDG_CONFIG_HOME` is
/// deliberately not read: no built-in wants the config root, and asking
/// for an input nothing consumes is a seam that can only rot.
pub(super) fn roots(env: &dyn EnvLookup) -> Result<Roots, harness_root::Error> {
    let override_v = env.get(ENV_LITANY_HOME);
    let xdg_data = env.get(ENV_XDG_DATA);
    let home = env.get(ENV_HOME);
    harness_root::resolve_from(
        override_v.as_deref(),
        None,
        xdg_data.as_deref(),
        home.as_deref().map(Path::new),
    )
}

/// The **followed config commit** of the calling agent's branch (ARCH
/// §2.2, [`workspace::current_config`]) — the same tip control resolves
/// from at every step boundary, so an accepted config edit reaches the
/// next call with no act per agent. The held arm (diverged lineages)
/// answers the fork commit, exactly as resolution does.
pub(super) fn followed_commit(
    workspace: &Path,
    branch: &str,
    git: &dyn GitRunner,
) -> io::Result<String> {
    let rev = workspace::agent_ref(branch);
    Ok(
        workspace::current_config::current_config(workspace, &rev, git)?
            .commit()
            .to_owned(),
    )
}
