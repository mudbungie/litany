//! One step's **request**, composed from the tree (ARCH §2.3, §5) — the
//! same machinery for both drivers (§6 *one step machinery, two
//! drivers*): the root exchange loop and the `litany advance` hop call
//! this with the system slot they composed, and neither can declare a
//! surface the other would not.
//!
//! The §5.2 head/body under the role's manifest rules, then the
//! transcript tail, is one code path for running, retry and replay;
//! everything the request declares (§3.3, §4.3, §2.7 — see [`tools`]) is
//! the role's elected tools, the compactor's injected pair, and the
//! closure over the assembled history — a compactor inherits the
//! dispatching branch's transcript (§2.3), so that last part is the
//! standing case, not an edge case; and a subtraction from the declared
//! surface is held until a miss is paid anyway (§5.5), the previous
//! step's record being the queue.

use super::{Resolved, assembler, canonical, prefix, tools};
use crate::prompt::{Deps, Error};
use brazen::CanonicalRequest;
use std::path::Path;

/// The composed request, and the orphan `tool_result` ids assembly
/// dropped composing its history (§2.3) — recorded on the step, never
/// acted on.
pub(super) struct Composed {
    pub(super) request: CanonicalRequest,
    pub(super) dropped_orphans: Vec<String>,
}

/// Compose step `step_seq`'s request for `agent_id` from `worktree`'s
/// read state, under `system` (the goal, identity and soul slot the
/// driver composed) and the boundary's `resolved` config.
pub(super) fn request(
    workspace: &Path,
    agent_id: &str,
    worktree: &Path,
    step_seq: u32,
    system: &str,
    resolved: &Resolved<'_>,
    deps: &Deps<'_>,
) -> Result<Composed, Error> {
    let assembled = assembler::assemble(worktree, resolved.manifest)?;
    let tools = tools::compose(
        worktree,
        &resolved.grant,
        &assembled.messages,
        &tools::injected(resolved.grant.role, deps.tool_executor, workspace, agent_id),
    )?;
    let request = canonical::build_request(
        resolved.model_id,
        agent_id,
        system,
        assembled.messages,
        tools,
        resolved.max_output_tokens,
        resolved.effort,
        resolved.priority,
    );
    let request = prefix::hold(
        prefix::previous(workspace, agent_id, step_seq)?.as_ref(),
        request,
    )?;
    Ok(Composed {
        request,
        dropped_orphans: assembled.dropped_orphans,
    })
}
