+++
title = "the injection design's open question about the compactor pair is answered downstream: DESIGN_TOOL_INJECTION §7 still lists three candidates"
created = 1788068631
updated = 1788068631
priority = 3
root_commit = "12899370c9ec7a5ed7f8e26d3d4fb914ea6c3310"
+++
`docs/DESIGN_TOOL_INJECTION.md` §7's fourth bullet records the seam inversion's
open question — where the compactor's procedure pair (`write_summary` /
`mark_for_deletion`, ARCH §2.7) belongs once the router is total — and states
that it is "filed downstream as yog bl-dfce, which is where it is adjudicated",
listing three candidates and declining to pick one.

**It has been adjudicated, so the bullet is now stale in the one way a design
doc must not be: it reads as open.** yog's operator ruled candidate 1 — the
host answers the pair itself as engine acts — and yog landed it against the
0.0.2 pin. The reasoning yog recorded, in its `docs/REMOTE.md` §5.4:

- The subject-locality invariant decides it alone (REMOTE §5, "a tool executes
  where its subject lives"). The pair's subject is the conversation:
  `write_summary` writes that conversation's own summary onto the compactor
  branch, `mark_for_deletion` nominates that same conversation's files. The
  conversation lives on the server, so no machine and no thrall is involved.
- REMOTE §12's "front door only" is not narrowed by this. It governs execution
  **on a machine**, which the pair is not — so the carve-out §7 worried about
  is not a carve-out in the invariant at all.
- The stated principle it falls out of: context management happens in the
  composing host.

**Nothing is asked of litany's code.** The surface needed already exists and is
the one yog used: the `tool` verb on the public `cmd::Command` surface — the
same in-process front door ARCH §3.3's third hop addresses as
`<driver_target> tool <name>`. yog re-enters it with the caller identity on the
child's environment (`LITANY_CONV_REPO` / `LITANY_CONV_BRANCH`, off
`RoutedCall`'s own `workspace` / `agent`) and the `tool_use` input on stdin, so
the compactor's semantics keep exactly one definition and it is this repo's.
Candidates 2 and 3 are therefore both unneeded: no procedure-injection
exemption, no re-shaping of the compaction act.

One detail worth a sentence somewhere, because a downstream host has to
rediscover it: the built-in resolves the calling agent's worktree from the
**process** environment, so an in-process `Command::Tool` call cannot carry a
per-call identity and the re-entry has to be a child. That is a property of the
front door, not a defect — it is what makes the identity harness-derived rather
than model-supplied — but it is the reason a host cannot answer the pair by
linking alone.

The ask: amend §7's fourth bullet to record the answer and the reasoning rather
than the three candidates, and cite the ruling's home (yog `docs/REMOTE.md`
§5.4). Whether ARCH §2.7 or §3.3 also wants a line noting that a host router
answers the pair by re-entering the front door is the doc owner's call.