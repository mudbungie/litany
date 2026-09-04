//! **The trim** — the one act that makes a forked tree exactly this
//! agent's context (ARCH §2.2, §2.3 step 2, §5.1), split from [`super`]
//! so the step-commit module reads as the commit it lands and the trim's
//! six parts sit beside the five modules that perform them.

use super::descriptors::Grant;
use super::{descriptors, reviewer_read, skill_bodies, unsettled};
use crate::prompt::Error;
use std::path::Path;

/// Stage the trim that makes the forked tree exactly this agent's
/// context (§2.2, §5.1) — one act with seven parts, each a no-op when
/// the fork point carried nothing to change, so the primitive is total
/// whatever ref it forked off:
///
/// 1. **Control leaves.** `manifest.yaml`, `workflow.yaml`,
///    `providers.yaml`, `version`, `souls/` — control is read from the
///    governing config commit, never from a worktree file (§2.2).
/// 2. **The config's skill bodies leave.** A lineage may carry
///    **workspace skills** at `skills/<name>/`
///    (`docs/DESIGN_LEARNING_LOOP.md` §3), and a body is not context
///    until it is *elected* — the fork inherits the description, and
///    `load_skill` fetches the body. [`skill_bodies::drop_config_bodies`]
///    removes exactly the commit's own names, never `skills/` whole,
///    because an agent's elected bodies live there too and §2.7 makes
///    them the compactor's input.
/// 3. **Descriptors are derived to the grant.** `descriptions/**` is
///    snapshotted whole into the governing config commit (one config
///    commit serves every role), and the agent's tree is the view of it
///    this role's `tools:` grants — checked out from that commit, not
///    inherited from whatever the fork point carried, so a child's
///    descriptors are never capped by its dispatcher's grant.
///    [`descriptors::derive`] does it, and declines a grant the commit
///    does not describe; see that module for the failures it closes.
/// 4. **The facts file is re-cut.** `facts.md` is the lineage's durable
///    memory ([`crate::facts`], §5.5) — derived from the governing
///    config commit at every fork, never from the dispatcher's tree, so
///    a fact authored today reaches every agent forked after it and no
///    running branch's prefix moves under it. Every role: a reviewer's
///    read of it is this part, not a second one.
/// 5. **A reviewer's workspace skills are read in fresh.** The config
///    commit's `skills/**` are checked out into a *reviewer's* tree and
///    nobody else's ([`reviewer_read::checkout`],
///    `docs/DESIGN_LEARNING_LOOP.md` §2): reading a proposable class
///    out of the commit the proposal will be parented on is what makes
///    a fresh read precede every write. The other class is part 4. The
///    same step records *which* commit that was, at the reviewer's read
///    mark (§3 step 4) — which is why the trim takes the agent's id: a
///    mark is keyed by it, and the commit that performs the read is the
///    one place that can state what it read.
/// 6. **The unsettled tool step leaves.** A tool-call dispatch forks
///    *during* the parent's tool step (§2.5), so a retained inherited
///    transcript — a fork-back-in root's own resumed conversation, or
///    the compactor's subject ([`super::inherited::prune_inherited_dialog`])
///    — can end in a `tool_use` block no `tool_result` entry answers — a
///    tail that settles on the parent's branch and never on the child's,
///    and that every provider refuses (§2.5 pairing).
///    [`unsettled::prune_unsettled`] removes exactly it; see that module
///    for the reproduced 400 it closes.
/// 7. **The name is settled.** `name` (ARCH §2.3, §2.11) is this agent's
///    display fact, and a fork inherits its fork point's — so the commit
///    overwrites it with the agent's own, or with nothing when the agent
///    is unnamed. Always a rewrite, never a deletion, for the reasons
///    [`crate::workspace::agent_name`] gives.
///
/// The parts are staged in this order because the later ones read the
/// worktree, and none sees another's writes. The one pair that shares a
/// path is deliberate and ordered — the skill-body drop reads the tree
/// before the reviewer read writes the commit's bodies back into it.
pub(crate) fn trim_to_context(
    worktree_path: &Path,
    agent_id: &str,
    grant: &Grant<'_>,
    name: Option<&str>,
    git: &dyn crate::template::GitRunner,
) -> Result<(), Error> {
    let mut args: Vec<&str> = vec!["rm", "-r", "-q", "--ignore-unmatch", "--"];
    args.extend_from_slice(crate::workspace::CONTROL_PATHS);
    git.run(worktree_path, &args).map_err(|source| Error::Git {
        op: "rm control files",
        source,
    })?;
    skill_bodies::drop_config_bodies(worktree_path, grant.config_commit, git)?;
    descriptors::derive(worktree_path, grant, git)?;
    crate::facts::cut(worktree_path, grant.config_commit, git).map_err(|source| Error::Git {
        op: "cut the facts file",
        source,
    })?;
    reviewer_read::checkout(worktree_path, agent_id, grant, git)?;
    unsettled::prune_unsettled(worktree_path, git)?;
    crate::workspace::agent_name::settle(worktree_path, name, git).map_err(|source| Error::Git {
        op: "settle the agent name",
        source,
    })
}
