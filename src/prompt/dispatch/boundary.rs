//! The §6 circumstances a step boundary derives from what the drain just
//! did — one evaluation, two drivers (ARCH §6 *the prompt→advance
//! collapse*): the `litany advance` hop and the root exchange loop both
//! call this at the same seam, so neither can fire a circumstance the
//! other does not.
//!
//! - **A delivered child result** — a result message the drain left in
//!   the inbox is interpreted by the returning child's role
//!   ([`child_result::interpret_pending`]: `deliver_result`,
//!   `land_compaction`, a gate-hold).
//! - **An operator spoke** — the drain delivered a message from the
//!   operator ([`drain::Delivery::operator_spoke`]), and the governing
//!   config's version fires `user_message` for it
//!   (`docs/DESIGN_PROCEDURES.md` §3; [`Version::fires_user_message`]).
//!
//! Both are read off disk and the drain's own report, never off a flag;
//! an empty inbox and a silent operator are the general path with empty
//! inputs.

use super::{child_result, drain};
use crate::config::version::Version;
use crate::config::{Event, Workflow};
use crate::prompt::workflow_actions;
use crate::prompt::{Deps, Error};
use std::path::Path;

/// Run the boundary's circumstances for `agent_id` — whose role, workflow
/// and config version are the boundary's own resolution — against the
/// `delivery` its drain just reported, under the executor lock the caller
/// holds and against the materialized `worktree`.
#[allow(clippy::too_many_arguments)] // one boundary, every fact it is judged on
pub(super) fn circumstances(
    workspace: &Path,
    agent_id: &str,
    worktree: &Path,
    delivery: &drain::Delivery,
    workflow: &Workflow,
    role: &str,
    version: Version,
    deps: &Deps<'_>,
) -> Result<(), Error> {
    child_result::interpret_pending(workspace, agent_id, worktree, workflow, deps)?;
    if delivery.operator_spoke && version.fires_user_message() {
        workflow_actions::run_step_hook(
            workspace,
            agent_id,
            worktree,
            workflow,
            version,
            role,
            Event::UserMessage,
            deps,
        )?;
    }
    Ok(())
}
