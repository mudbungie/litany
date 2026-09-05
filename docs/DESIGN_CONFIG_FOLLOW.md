# Design: follow-the-tip — the conversation is not pinned to its fork config (bl-403b)

**Status:** living document. Deliverable of bl-403b. This document numbers no
sections of its own beyond the headings below, so a bare `§N` is always a
section of `docs/ARCHITECTURE.md` (the cross-document reference rule, §2.1).

**Ruling being implemented (operator, 2026-09-01, extending bl-f928's).** "The
conversation should not be pinned! configuration should be changeable at any
time, on any turn." The whole configuration follows the workflow's precedent:
a conversation resolves the workspace's *current* config at every step
boundary by default, and fork-time pinning is gone as the default behavior. A
step in flight finishes on the config it started with.

**The live evidence.** A deployed workspace's roles moved off an
uncredentialed provider row onto a working one, and every existing
conversation kept refusing on the dead row until each was hand-retargeted and
nudged. The operator had no idea their running conversations were pinned to a
dead config, and nothing on any surface said so.

---

## 1. What fork-time pinning actually bought — stated honestly

The pre-ruling rule ("fork is the freeze", §2.2) resolved all control from
the **governing config commit** — the nearest `config/*` ancestor of the
agent's branch, an ancestry query that never moves. It bought exactly two
things:

1. **Mid-conversation stability.** An agent's policy could not change under
   it without a per-agent act. The ruling holds that a `litany config` edit
   *is* the intended act, and that its reach — every running conversation on
   the lineage, at its next step — is the point, not the hazard. The incident
   above is what the "stability" cost in practice.
2. **Branch-only policy replay.** "Which config governed step N" was
   derivable from the branch's ancestry alone, at any later date. Under
   follow-the-tip it is a fact about *when the step ran*. What survives: the
   step record carries the policy's byte-for-byte *effects* (`request.json`,
   §2.3 — model id, soul, tools, workflow-derived retry all ride it), and an
   archive carries the lineage heads as of archiving (§9.2), so a replayed
   workspace resolves the archived tips. What was filed rather than
   lost silently has since **shipped** (bl-e4a0): each step's `meta.json`
   records the config commit the step resolved control from, and beside it
   the commit its `workflow.yaml` came from — the two differ exactly when a
   workflow mark stood (bl-f928). Exact per-step policy provenance is back,
   as a diagnostic fact rather than a control input (§2.3).

Mid-*step* consistency was never the freeze's job and is untouched:
resolution is per-hop (§6 "no resident interpreter"), so a step in flight
finishes on the commit it resolved.

## 2. The inversion: the followed config commit

`workspace::current_config` derives the **followed config commit**, and
resolution (`resolve_worker`, for the Fork and Agent sources alike) reads all
control from it:

> governing commit (the unchanged ancestry query) → the config heads whose
> history contains it → their **distinct tips**. Exactly one distinct tip —
> the single-lineage case, and equally the freshly-forked-variant case where
> several refs still stand on one commit — is **followed**. Two or more
> distinct tips is real divergence the derivation must not guess between: the
> **fork commit itself** resolves (the pre-ruling answer), the resolver says
> so loudly at every step (`litany: notice: N diverged config lineages reach
> […]`), and `litany retarget` settles the lineage. Zero cannot occur — the
> head that contributed the governing commit contains it.

One rule, no fresh-start special case (an un-advanced lineage's tip *is* the
fork commit), and the notice is precisely the surface the motivating
incident lacked. The same derivation is read by the §6 dispatch budget gate
and the dispatch-time role-validity check, so fork-time artifacts, gate
ceilings and step-time resolution stay one answer.

## 3. Retarget's disposition: re-scoped, not deprecated

Two mechanisms no longer mean one thing; each keeps one meaning:

- **Follow-the-tip** is the temporal default *within* a lineage: no verb, no
  act, no migration.
- **`litany retarget`** is the change of **lineage**: it re-forks the
  branch's ancestry onto the target lineage's head so the follow derivation
  follows that lineage from then on, and it is the act that settles a
  diverged (held) lineage. A target the agent already resolves is a clean
  no-op — which is now what retargeting your own lineage's advanced head is.
  Its mechanics (re-derived dispatch commit, rebase-forward, the child
  transfer price) are unchanged.
- **The workflow mark** (bl-f928) stays the scoped per-agent override and
  composes as designed: mark wins over tip for the workflow fact — a switch,
  and now also the deliberate way to *hold one agent out* of a lineage-wide
  change.

## 4. What still pins, and why

- **A step in flight** — one hop's resolution (structural, §6).
- **The workflow mark** — a deliberate per-agent pin (bl-f928).
- **Dispatch-time tree artifacts** — the `goal.md`/`soul.md`/`name`
  copies are the fork's. The wire soul already follows (the step composes
  it from resolution, never the tree copy). The `descriptions/**` cut no
  longer pins: since bl-37cd it is re-derived at every step boundary
  against the commit and grant that just resolved (below).
- **Diverged lineages** — held on the fork commit, loudly, until retargeted
  (the conservative arm of §2, never a guess).

## 5. Shipped by bl-403b, and what is deferred

Shipped: `src/workspace/current_config.rs` (the derivation + tests);
resolution, the dispatch budget gate, the role-validity check and retarget's
no-op comparison read it; the per-step divergence notice; ARCH §2.2/§3.4/§4/§6,
PRINCIPLES, TAXONOMY and README amendments; every freeze-pinning test
inverted to pin the ruling (never deleted — each asserts the followed
behavior its predecessor denied).

Shipped since, by bl-e580: the in-process root loop resolves per step too, so
"at every step boundary" now holds for both drivers with no exception. A
config edit landing during a fresh root's first exchange governs that
exchange's next step — the loop calls `resolve_worker` at the top of each
iteration and re-reads the soul, the model binding, the retry policy, the
budgets, the grant and the manifest rules with it. Step 1 keeps the fork
resolution the start already took, which is step 1's own boundary.

Shipped since, by bl-e4a0: per-step config provenance in `meta.json`.
`resolve_workflow` hands back the commit it read the workflow from beside
the workflow itself; `WorkerConfig` and `Resolved` carry it next to the
config commit the grant already named; both step-loop drivers write
`config_commit` and `workflow_commit` into every step record. Both are
always written, equal on the unmarked path — so their disagreement is the
record that a mark stood, and a reader never has to tell "no mark" from
"not recorded". Neither is re-derived after the fact: a mark is a ref an
operator can rewrite between the resolution and the question.

Shipped since, by bl-37cd: **the descriptor cut follows the tip too.**
`descriptions/**` was cut to the role's grant at the dispatch commit and
never again, so a followed tip that *widened* a grant left the agent
calling a tool nothing in its tree described, and one that *revoked* a
grant left a convincing schema on disk for a tool the wire no longer
declared — which is the exact failure the cut exists to close (yog
bl-55b1), one config edit later.

The refresh is the cut, re-run: `descriptors::refresh` calls the same two
halves the fork does, and lets **git** answer whether anything moved
(`git status --porcelain -- descriptions`), so there is no
change-detection, no record of which commit the tree was last cut from,
and no special case for the boundary where nothing changed. It commits
what moved, and it commits **before** the read-state capture — the wire
reads descriptors off the worktree while replay re-assembles against
`meta.json`'s `commit` (§2.10), so a worktree-only refresh would put
bytes on the wire that no replay could reproduce. That is the same shape
and the same moment as the boundary's other landing acts, the inbox drain
and the child-result interpretation.

Three decisions came with it. **It re-cuts but does not re-decline**: the
fork refuses a grant the config commit does not describe, before a branch
exists; at a boundary the agent already exists, and killing a running
conversation over an operator's edit is the very failure class this
ruling was made to fix — so an undescribed tool is left out of the tree
with an operator notice, and `tools::compose`'s intersection drops it as
it drops any absent schema. **Undescribed is not revoked**: the drop half
reads the grant *whole*, so a tool still granted but no longer described
keeps its bytes (a config disagreement must not destroy the only
surviving description), while a tool the tip removed from `tools:` loses
its stale copy, which is the revoke half. And **step 1 does not refresh**
— its boundary is the fork the caller already resolved against, and the
dispatch commit made this very cut from that answer, the same carve-out
the per-step resolution already has.

One neighbour moved with it: a reviewer's proposal reads *what the
reviewer changed* as its founding commit against its terminal ref, and
now excludes `descriptions/**` beside `messages/**` and `summary/**`
(`child_result::proposal::edits`) — all three are harness-written on the
branch, so none is a reviewer's edit. Without that, a tip moving
mid-review would refuse the whole proposal as *Outside*, precisely when a
review is most worth having.
