# Design: the workflow mark — switching an agent's workflow on the fly (bl-f928)

**Status:** living document. Deliverable of bl-f928. This document numbers no
sections of its own beyond the headings below, so a bare `§N` is always a
section of `docs/ARCHITECTURE.md` (the cross-document reference rule, §2.1).

**Ruling being implemented (operator, 2026-08-31).** The engine operates by
**workflows** — named declarations of what happens at every step: the basic
agentic loop, compaction, when to do what. A workflow determines the next
step, so it must be **switchable on the fly**. Ship the mechanism plus a
**basic agentic loop** default that does exactly what the engine does today;
a second workflow must be cheap to add and safe to switch mid-conversation,
because the point is to test and optimize alternative workflows against each
other.

**Terminology (AGENTS.md terminology discipline).** No term is coined here.
The operator's "workflow" and litany's existing §6 term of art converge —
`docs/TAXONOMY.md` §1 records the reconciliation — and this document adds two
companion names defined below: the **workflow mark** and the **basic agentic
loop**.

---

## 1. Survey: where "what happens next" lives today

The next-step decision is already split along a deliberate line (§6):

- **The workflow** — the config's `workflow.yaml`: event→action bindings
  (lifecycle events, per-step hooks), the `compaction:` checkpoint clock, the
  `retry:` policy, `budgets:`, `tool_output:`, `tool_control:`. It is
  interpreted afresh at every hop ("no resident interpreter", §6) and §6
  names it "the primary surface for experimentation"; §9.3 experiments are
  precisely workflow-config variants run against the task suite.
- **The interpreter's spine** — warrant derivation, the pairing invariant,
  the exec baton, the closed action set. Code, by the §6 severability line:
  "the closed set is the interpreter's vocabulary, and experiments recombine
  that vocabulary without touching the harness."

So the *artifact* the ruling asks for exists, and it is already consulted per
step. What fails the ruling is the switch. The workflow is frozen at fork —
"an agent's workflow is fixed for its life" (§6, now amended) — and the one
exit, `litany retarget` (§2.2), is a **re-fork**: a re-derived dispatch
commit, a rebase-forward of the whole branch, and, for a child, the price of
its work-product transfer. That is a migration. The ruling's test is that
switching is *just changing which workflow is consulted* — and since the
workflow is consulted per step, the design must make "which one" a per-step
question with a switchable answer.

## 2. The design: the workflow mark

**`refs/litany/workflow/<agent-id>`** — a **standing** per-agent ref naming
the config commit whose `workflow.yaml` governs the agent from its next step
boundary on. Resolution (`resolve_worker`) answers the workflow question with
one derivation:

> the `workflow.yaml` of the **nearest workflow mark on the agent's descent**
> (its own id first, then each ancestor by `parent_of`), else of the
> **governing config commit** — today's path, byte-for-byte.

Everything else — soul, `providers.yaml`, `manifest.yaml`, `descriptions/**`,
grants, `version` — still resolves from the governing config commit. The mark
switches the what-happens-next policy alone: exactly the contents of
`workflow.yaml`.

Why this shape falls out of the architecture rather than being bolted on:

- **Switching needs no landing, no rebase, no restart.** Every hop is a fresh
  image resolving control from disk (§6 "no resident interpreter"), so the
  switch is effective at the agent's next step boundary by construction —
  writing a ref *is* the switch. Timing is retarget's own rule: a workflow
  governs steps, never mid-step.
- **One home for the fact.** The mark stores no workflow content — content
  lives only in config commits, authored only by `litany config` (§2.2). The
  mark selects *which commit answers*, and "which workflow governs agent X"
  stays a pure disk query (PRINCIPLES, single source of truth; §6 "workflow
  position is derivable from disk" is untouched — the mark is policy
  selection, not position).
- **Standing, not consumed.** Retarget's mark is a one-shot request ("re-fork
  now") and is consumed at the boundary. The workflow choice is standing
  policy — a consumed mark would re-freeze at the next boundary. So it joins
  `abandoned` as a non-derivable policy assertion in the shared mark
  namespace (`refs/litany/`), reaped with the agent by `litany delete`,
  archived with the refs (§9.2), crash-safe because it is a ref.
- **Inheritance by descent.** The nearest-mark walk means marking the root
  switches the conversation's whole tree — children dispatched before or
  after alike — without touching any branch, and a child's own mark overrides
  its ancestors'. This mirrors governing-config's nearest-ancestor semantics
  and is a pure derivation from the id string (§2.3 hyphenated descent) plus
  refs. The §6 dispatch budget gate reads the same derivation (the
  dispatching branch's nearest mark), so the ceiling a fork is refused under
  and the ceiling the child's own steps check are one answer, not two.
- **A pin as well as a switch.** A standing mark holds its workflow against
  later retargets of the same agent: retarget moves the governing lineage,
  and the mark still overrides it. Deliberate — the mark is the more specific
  assertion, and clearing it is one act.

**The verb.** `litany workflow <workspace> <agent> [--config <name>]` writes
the mark at `config/<name>`'s head (`<name>` defaults to `default`);
`--clear` deletes it, returning the agent to its governing config's workflow
— removing the mark deletes config, never code (PRINCIPLES severability).
Validity precedes the mark (the retarget discipline): the workspace, the
agent, and the lineage must exist, and the target head's `version` (§10) and
`workflow.yaml` (closed vocabulary) must parse. The `dispatch(<role>)`
cross-check stays at resolution, where the marked workflow meets the
governing `providers.yaml` and an undeclared role is declined with a named
error before the first model call, exactly as today (§4.3). Marking is
otherwise unconditional — last write wins, and a mark naming the commit
already governing is behaviorally identical to no mark, so no no-op arm
exists to drift.

## 3. The basic agentic loop default

The shipped `template/workflow.yaml` — the declaration every workspace's
`config/default` freezes at `litany new` — **is** the default, and it is
hereby *named*: the **basic agentic loop**. Its content is exactly today's
stock behavior (dispatch → step loop → deliver results, compact every 20
commits, retry 3, 16 KiB tool-output bounds, nothing bounded by budgets, no
tool control), and this design changes not one byte of it. Equivalence with
"what the engine does today" is therefore pinned by the entire existing
suite: an unmarked agent takes the identical code path and reads the
identical bytes.

Adding a second workflow is config: `litany config <ws> <alt> --from
default`, edit `workflow.yaml`, then `litany workflow <ws> <agent> --config
<alt>` to switch any live agent onto it — and `--config default` (or
`--clear`) to switch back. An A/B experiment is two lineages and a mark.

## 4. Attacks considered

- *"Two exits from the config freeze?"* Yes, deliberately scoped: retarget
  moves the **whole config** and rewrites ancestry (needed when the model id
  or soul must change); the workflow mark moves the **workflow fact alone**
  and rewrites nothing. They compose (above, "a pin as well as a switch").
- *"Why not switch via retarget?"* A retarget is a re-fork: it pays a
  rebase-forward, re-derives the dispatch commit, and costs a live child its
  transfer (§2.2). For the one fact that is consulted fresh at every hop, all
  of that is machinery the switch does not need — the ruling's own test
  ("just changing which one is consulted") selects the mark.
- *"Why not a new sidecar or a worktree file?"* A worktree copy of a config
  fact is the second-home drift §2.2 removes control files to prevent, and it
  would be editable by the agent's own `bash`. A ref is outside every
  worktree, user-written, and already the established pattern.
- *"Doesn't §6 say the workflow is fixed for life?"* It did; the operator
  ruling amends it. §6 now states the freeze holds **by default** and names
  the mark as the workflow fact's own exit, beside §2.2's retarget.

## 5. Shipped by bl-f928, and what is deferred

Shipped: `src/workspace/workflow_mark.rs` (the ref: read/write/clear);
`src/prompt/resolve/workflow_source.rs` (the derivation: nearest mark by
descent, §10 version guard on a marked commit, else governing);
the §6 dispatch budget gate reading the same derivation
(`child_dispatch::budgets`); `src/cmd/workflow.rs` (the verb); ARCH §6 and
TAXONOMY amendments; tests in each seam plus an end-to-end switch on the
stub-adapter harness.

Shipped since, by bl-fcd8: the named default is a file. `litany prime` seeds
`<config-root>/workflows/basic-agentic-loop.yaml` (`src/install.rs`), reading
the embedded `template/workflow.yaml` the `litany new` freeze already reads,
so the pool entry and the freeze are one asset and cannot become two
declarations of one default. Seed-if-absent like every other pool entry, so a
curated file survives. Before it, §3's "adding a second workflow is config"
started from an empty directory: the default the ruling named had no file to
copy or fork from.

Deferred, filed: the in-process root loop (`run_exchange`) resolves once per
exchange, so a mark written *mid-exchange* takes effect from the next
advance-driven step — the same latency retarget already has there; dissolved
by the §6 prompt→advance collapse (bl-e580). Surfacing the
derivation in `litany scan` (bl-5c02). The eval A/B driver (bl-f838).
