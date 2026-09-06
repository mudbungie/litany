---
name: remember
description: Record one durable fact about this workspace, so that a brand-new conversation started here knows it without being told. This is the only durable memory you have and the only write you can make to the workspace's configuration; nothing else you do outlives your own conversation. Use it when the user tells you to remember something, and when you learn a standing fact about this workspace that a later conversation would otherwise have to rediscover. Your write is staged as a proposal and takes effect when the operator accepts it — say so when you report back.
---

# remember

Appends one fact to this workspace's durable facts file, `facts.md`
(ARCH §5.5, `docs/DESIGN_CONTEXT_ECONOMY.md` §3). That file is pinned
into the context of every conversation started on this workspace
afterwards, so a fact recorded here is known to a later conversation with
no tool call and no file read.

## Input

```json
{ "fact": "<one or two sentences>" }
```

Write the fact so it still makes sense to a conversation that has none of
this one's context: name the thing it is about rather than saying "it" or
"the above".

## Output

```json
{
  "status": "proposed",
  "proposal": "<your agent id>",
  "accept": "litany proposal <workspace> <your agent id> --accept"
}
```

- `status: proposed` — the fact is staged.
- `status: already_recorded` — that fact is already in `facts.md` or in
  your standing proposal; nothing changed and nothing was staged.

## It is staged, not written — say so

Your write does **not** take effect on its own. It lands as one commit on
the branch `proposal/<your agent id>`, which no configuration lineage
points at until a person runs the `accept` command the output hands you.
That is deliberate: the file governs every future conversation here, so
the last word is the operator's.

When you report back, say what you recorded, that it is staged, and the
command that settles it. Do not tell the user the fact is already in
effect.

## Calling it more than once

Each call adds to the **same** proposal rather than opening a new one, so
remembering three things leaves the operator one thing to read and
accept. Order is the order you called in.

## When to use

- The user asks you to remember, note, or record something.
- You learn a standing fact about this workspace — a convention, a
  retention figure, where something lives, a decision already taken —
  that a later conversation would otherwise have to rediscover.

## When not to use

- Anything true only of this conversation. The transcript holds that.
- Reference material, procedures, or anything long. The file is capped at
  4096 bytes for the whole workspace; it is the small always-present
  memory. Long procedures belong in a skill, and reference material in a
  file you write and read on demand.
- Anything you are not confident is true. A wrong fact here is repeated
  to every later conversation.

## Failure modes

- An empty `fact` → exit non-zero, `is_error: true`. Say the fact.
- The file would exceed its cap → exit non-zero naming the size and the
  cap. Nothing is staged and a proposal you already had is untouched.
  Shorten the fact, or ask the operator to prune `facts.md`.

There is no other way to write the configuration lineage from inside a
conversation: `litany config` and `litany proposal --accept` refuse when
run from a tool call, whatever shell you reach them from. If what you
want is not a fact — a change to a soul, a grant, a model or a skill —
say so in your answer and let the operator make it.

Every result is a §3.3 **result envelope**: an `Exit code: N` line
first, then the output described above, then — whenever the tool wrote
any, on success as well as failure — its stderr under a
`--- stderr ---` marker.
