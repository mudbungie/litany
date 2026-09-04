//! `litany tool <name>` — in-process built-in tool entry (ARCH §3.3):
//! `tool_use.input` JSON on stdin, bytes on stdout, exit 0/non-zero. The
//! stdio arrives through [`Fx`](super::Fx) (locked by the binding), as
//! do the injections a built-in may need
//! ([`builtin::Bindings`]): the driver target the `dispatch` /
//! `message` built-ins and a program's stub module re-enter the front
//! door with (§2.11 — the same injected target the §3.3 tool resolver's
//! third hop addresses), and the adapter target, stop flag and tool
//! injection a program's toolset resolution runs under.

use super::{Error, Fx, Outcome};
use crate::prompt::tool::builtin;

/// `litany tool <name>`.
#[derive(clap::Args, Debug)]
pub struct Args {
    // The help text is rendered from [`builtin::NAMES`], the same list the
    // unknown-tool decline names — one pool, two surfaces. The name is *not*
    // a `value_parser` over that set: the compactor pair (`write_summary` /
    // `mark_for_deletion`, §2.7) is routed but unadvertised, so a clap
    // possible-values gate would refuse the compactor's own re-entry, and
    // clap's parse error would replace the §3.3 decline (its stderr carried
    // into `tool_result.content`) with its own voice and exit code.
    #[arg(help = name_help())]
    pub name: String,
}

/// The `<NAME>` argument's help line — the built-in pool, named.
fn name_help() -> String {
    format!("Built-in tool to run; one of: {}", builtin::pool())
}

/// Delegate to [`builtin::run`] over the injected stdio; the process
/// exit code rides back as [`Outcome::Code`](super::Outcome::Code)
/// (§3.3). The failure prefix is `tool <name>`, as today.
pub fn run(args: Args, fx: &mut Fx) -> Result<Outcome, Error> {
    let bindings = builtin::Bindings {
        driver_target: &fx.driver_target,
        adapter_target: fx.adapter_target.as_deref(),
        stop: fx.stop,
        injection: fx.tool_injection,
    };
    let code = builtin::run(
        &args.name,
        &bindings,
        &mut fx.tool_stdin,
        &mut fx.tool_stdout,
        &mut fx.tool_stderr,
    )
    .map_err(|e| Error::new(format!("tool {}", args.name), e))?;
    // Tool exit codes ride within `u8` (POSIX), so `as u8` is faithful.
    Ok(Outcome::Code(code as u8))
}
