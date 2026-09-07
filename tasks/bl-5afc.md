+++
title = "no plan mode, while the mechanism for one ships unbound: gate_return_on plus the declared-but-unbound reviewer role is a better answer nobody will configure"
created = 1788673627
updated = 1788745926
claimant = "Cantaloups-M2"
priority = 4
root_commit = "12899370c9ec7a5ed7f8e26d3d4fb914ea6c3310"
tags = ["usability-r1"]
+++
Round 1, lane comparator.

Both codex and Claude Code ship a plan mode as a first-class, one-keystroke
state: codex's slash command "switch to Plan mode", Claude Code's
--permission-mode plan (a peer of acceptEdits, auto, bypassPermissions, manual
and dontAsk). hermes has none, and offers a todo tool plus a bundled plan skill
that writes markdown under .hermes/plans/ as the nearest thing.

litany has none either, and the verb set (src/cmd/mod.rs) carries no
propose-then-execute mode for an agent's own work.

The interesting part is that litany's answer is better than a mode and nobody
will ever configure it. ARCH §6 ships gate_return_on(predicate) in the closed
action set and documents the exact binding:

    worker_return: [dispatch(verifier), gate_return_on(verifier.approve)]

which holds the worker's delivery commit until a verifier's terminal response
leads with APPROVE. ARCH §4.3 makes the role set open, so adding a verifier is a
config commit with zero code. And template/providers.yaml already declares a
third role, reviewer, "declared and unbound", precisely so that adopting the
learning loop is a config edit rather than an authoring act.

So the mechanism, the role and the binding all exist, and what ships by default
is basic-agentic-loop.yaml with none of them wired. Two named workflows ship;
one is the default and the other (learning-loop.yaml) is seeded but never
defaulted to.

Why this is a comparator gap and not a preference. A plan mode is the first
thing a new user reaches for on a task they do not yet trust the agent with, and
it is reached for BEFORE they have learned what a workflow binding is. The
comparators put it one keystroke away; here it is a config edit whose existence
is documented in an architecture spec.

Proposed shape, in the order that costs least. (1) Nothing new: name the
capability where a user will meet it. The seeded learning-loop.yaml is the
answer to "can I make it check its work first" and nothing at the surface says
so — a line in the template's comments and in README's quickstart is the whole
change. (2) One rung up, and this is the yog half rather than the litany half:
a gesture that switches a conversation's workflow mark is already shipped
(litany workflow, writing refs/litany/workflow/<agent-id>, nearest mark on the
descent wins, no migration and no restart), so "plan mode" at the seat is that
mark plus the seeded workflow — a binding, not a mode. Worth confirming that
reading is right before anything is built, because if it is, the feature is
already there and only the noun is missing.

What NOT to do: add a mode to the loop. ARCH §6's action set is closed both ways
(spawn_root_agent and spawn_exchange were subtracted and are declined at parse),
and a plan mode as core state would be a second home for what gate_return_on
already expresses.

Filed p4: a default and a name, not a feature.