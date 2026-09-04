//! `litany invoke` — the front door for one **inner invocation**
//! (`docs/DESIGN_CODE_EXECUTION.md` §2.1, ARCH §3.4).
//!
//! Stdin is one `tool_use` block, `{id, name, input}` — the same object
//! a tool control reads (ARCH §3.3 *Tool control*). The invocation runs
//! through the door's gates and the executor, its **raw** result
//! envelope goes to stdout, and the process exits with the tool's own
//! exit code. Nothing is committed and no transcript entry is written:
//! what the model reads is the output of the tool that composed this
//! invocation, never the invocation itself.
//!
//! Why a verb rather than a pipe between the composing tool and the
//! window: `docs/PRINCIPLES.md` *Everyone uses the front door* — "no
//! in-process sidechannel, no ad-hoc socket". A verb also makes a
//! composing program testable against a fixture door.
//!
//! It takes no arguments. Whose invocation this is arrives in the §3.3
//! stdio contract's environment, as it does for every built-in, so a
//! composing tool need pass nothing but the block itself.

use super::{Error, Fx, Outcome};
use crate::prompt::dispatch::door;
use crate::prompt::tool::builtin::dispatch::ProcessEnv;

/// `litany invoke` — no arguments; the block is stdin.
#[derive(clap::Args, Debug)]
pub struct Args {}

/// Run one invocation over the binding's injected stdio, driver target,
/// adapter target, stop flag and tool injection ([`Fx`](super::Fx)).
/// The tool's exit code rides back as
/// [`Outcome::Code`](super::Outcome::Code), the same contract
/// `litany tool` ends on (§3.3).
pub fn run(_args: Args, fx: &mut Fx) -> Result<Outcome, Error> {
    let code = door::cli::run(
        &ProcessEnv,
        &mut fx.tool_stdin,
        &mut fx.tool_stdout,
        &fx.driver_target,
        fx.adapter_target.as_deref(),
        fx.stop,
        fx.tool_injection,
    )
    .map_err(|e| Error::new("invoke", e))?;
    // Tool exit codes ride within `u8` (POSIX), so `as u8` is faithful.
    Ok(Outcome::Code(code as u8))
}
