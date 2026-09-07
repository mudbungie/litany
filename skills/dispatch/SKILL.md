---
name: dispatch
description: "Start a child agent on a goal you supply. It runs on its own branch and returns its result as a deposit into your inbox, delivered at one of your later step boundaries. Use it for work that is genuinely separable — a focused investigation whose reading would crowd this branch, several independent lines you want running at once, a goal needing a role you do not hold. It is not a way to pass on the goal you were given; that goal is yours to execute, and reporting that you dispatched is not answering it. Never wait on a child by sleeping or re-checking — there is nothing to poll, and ending your step is how you wait: the child's result revives you."
---

# dispatch

Starts a child agent off the current step, on a fresh branch off this
branch's tip. The child receives the goal verbatim and runs against the
role's soul; you get back its address and continue. Parallelism is
expressed by issuing several dispatches in one step; each child's result
lands as its own message on a later step as it completes (ARCH §2.5,
§2.11).

## Input

```json
{ "role": "<role-name>", "goal": "<goal text>", "name": "<display-name>" }
```

- `role` — the child's role. The role set is open: any role defined in
  this repo's `providers.yaml` (`roles:` block) with a soul at
  `souls/<role>.md` is dispatchable, and nothing enumerates role names.
  A role missing either half is refused before a branch exists.
- `goal` — the goal text. Pinned at the head of every model call on the
  child's branch and not rewritten during execution.
- `name` — display name for the child: one unbroken word, unique among
  the workspace's living agents, set here and never rewritten. A name is
  the child's identity in every surface — it is what `message` accepts
  in place of the child's id, the child is told it, and it labels the
  child in the tree — so supplying one keeps subagent identities and
  tasks clear; pick a name that says what the child is for. Omit it and
  a valid two-word PascalCase name is minted automatically
  (`PeachHollow`) — never an error. A
  supplied name that is malformed, begins like an agent id, or is
  already worn is refused and no child is created.

## Output

A JSON object on `tool_result.content`:

```json
{ "status": "in_progress", "handle": "<child-agent-id>" }
```

Despite the field name, `handle` is not something to poll: it is the
child's **address** — its full hyphenated descent (`<your-id>-<sub-id>`),
which is also its branch name and what shows up in the git tree on the
UI. A dispatch returns the address, never the result, and there is
nothing to await (ARCH §2.5). The result arrives on its own channel: a
deposit into your inbox, delivered at a later step boundary (ARCH
§2.11).

## Waiting

**Do not sleep and do not poll.** There is no result to fetch, no status
to re-check, and no command whose only job is to let time pass. Every
such step is a paid model call that learns nothing.

Make whatever other progress you have. When you have none, **stop
emitting tool calls**. A deposit into a quiescent agent starts a driver
(ARCH §2.11), so the child's return revives you on this same branch with
your context intact — you are parked exactly as a root agent is parked
awaiting the user. Parking is parking; ending a step is how you wait,
not how you give up.

Parking has one cost worth knowing before you choose it: a step that
emits no tool call is your own terminal event, and what you said in it
is returned as your result (ARCH §2.6). So park on an outstanding child
only when what you have to say is worth returning. If the only thing
left for you to do was wait, you did not need the child.

## When to use

- A subtask is large enough that finishing it inline would crowd this
  branch's context — push it to a child, get back its terminal response
  later.
- Several independent investigations can run in parallel — issue N
  dispatches in one step; each result lands as its own message as it
  completes.
- A goal needs a different toolset or model than this role has — pick
  the role that fits and dispatch there.

## When not to use

- **To pass on the goal you were given.** The goal is yours to execute.
  Dispatch buys separation or parallelism; it does not buy an escape
  from the work. If you hold the tools to do it, do it — and reporting
  that you dispatched is not answering a goal (ARCH §2.6: your terminal
  response *is* your result).
- A short read or computation that fits inline — `read_file` and
  `bash` are cheaper than a whole child branch.
- A goal that depends on this branch's in-flight reasoning — the child
  sees only the goal text and its branch's tree, never your prior steps,
  so each hand-off pays a fresh agent to rediscover what you already
  know.

## Notes

- The harness writes the goal to `goal.md`, the role's soul to
  `soul.md`, and the display name (supplied or minted) to `name` on the
  child's first commit; the child's worktree is at
  `<workspace>/agents/<child-agent-id>/` (ARCH §2.2).
- Dispatch is gated by the workflow's `budgets:` at the fork, so a
  dispatch that would breach `max_depth` — or start work under a tree
  that has already spent its tokens or wall — is refused outright and
  leaves no branch behind (ARCH §6). The shipped config declares
  `max_depth: 5` — a root plus five levels of delegation — and no spend
  ceiling, so out of the box a dispatch is refused only for being too
  deep, and a refusal names the axis.
- Stopping a child is a separate user action through the UI; there is
  no `stop_dispatch` tool in v0.4.
