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
   workspace resolves the archived tips. What is filed rather than lost
   silently: recording the resolved config commit in each step's `meta.json`
   (bl-e4a0), which restores exact per-step policy provenance as a
   diagnostic fact.

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
- **Dispatch-time tree artifacts** — the `descriptions/**` cut and the
  `goal.md`/`soul.md`/`name` copies are the fork's. The wire soul already
  follows (the step composes it from resolution, never the tree copy); the
  descriptions refresh under a tip that changes a grant is filed (bl-37cd).
- **Diverged lineages** — held on the fork commit, loudly, until retargeted
  (the conservative arm of §2, never a guess).

## 5. Shipped by bl-403b, and what is deferred

Shipped: `src/workspace/current_config.rs` (the derivation + tests);
resolution, the dispatch budget gate, the role-validity check and retarget's
no-op comparison read it; the per-step divergence notice; ARCH §2.2/§3.4/§4/§6,
PRINCIPLES, TAXONOMY and README amendments; every freeze-pinning test
inverted to pin the ruling (never deleted — each asserts the followed
behavior its predecessor denied).

Deferred, filed: the in-process root loop resolves once per exchange, so a
config edit mid-*first*-exchange lands from the next advance-driven step —
bl-e580, re-scoped to this ruling. The stale dispatch-time descriptions cut
under a grant-changing tip — bl-37cd. Per-step config provenance in
`meta.json` — bl-e4a0.
