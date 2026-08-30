---
name: message
description: Deposit a message into an existing agent's inbox — the user, your parent, a child you dispatched, a sibling, or yourself. Address the recipient by its agent id or by its unique display name. The message is delivered at the recipient's next step boundary; if the recipient is quiescent, the deposit wakes it. Use to steer a running child, report upward to a parent, or leave yourself a note — content addressed to an agent that already exists, not a new dispatch.
---

# message

Deposits content into an existing agent's inbox (ARCH §2.11). Unlike
`dispatch`, it starts no new agent and creates no branch — the recipient
must already exist, addressed by its agent id or by its display name.
Delivery happens at the recipient's next step boundary; a message to a
quiescent agent revives it. The deposit is synchronous and returns
immediately.

## Input

```json
{ "agent": "<agent-id-or-name>", "content": "<message text>" }
```

- `agent` — the recipient, addressed either way:
  - **by id** — its full hyphenated descent, which is also its branch
    name. A child's id is the handle its dispatch returned; your
    parent's id is your own id minus its last segment; your own id is a
    legal recipient (a note to self).
  - **by display name** — the name an agent was dispatched with, if it
    has one. It resolves only when exactly one living agent wears it; a
    name worn by two is refused, naming the candidate ids, and never
    guessed. Names and ids never collide: a name can never begin like an
    id.
- `content` — the message body, delivered verbatim as a user-role
  entry in the recipient's transcript.

## Output

A JSON object on `tool_result.content`:

```json
{ "status": "deposited" }
```

The deposit landed. There is nothing to poll and no address to capture
— the message either drains at the recipient's next boundary or wakes a
quiescent recipient (ARCH §2.11).

## When to use

- Steer a child you dispatched while it is still working — a course
  correction, a new constraint, a piece of context it now needs.
- Report upward to a parent as a long-lived child (a watchdog, a
  reminder, an adversarial critic running alongside).
- Leave yourself a note your own next step will see.

## When not to use

- Starting new work toward a goal — that is `dispatch`, which spawns a
  child agent and returns its address.
- Rewriting an agent's goal — a message adds context *beside* the
  pinned goal (ARCH §2.8); it does not replace it.

## Notes

- The sender recorded on the message is your own agent id, taken by the
  harness from `LITANY_CONV_BRANCH` — you cannot forge it, and the
  recipient treats every sender uniformly (ARCH §2.11).
- Delivery is deferred to a step boundary, never mid-step: a message
  cannot interrupt in-flight work (stopping is a separate user action).
- The message is delivered once and becomes an ordinary transcript
  entry the recipient (or a compactor) may later curate away.
- **A reply is a deposit like any other, and you wait for it the same
  way you wait for a dispatched child: by ending your step.** Do not
  sleep and do not re-check — there is nothing to poll here either. Make
  whatever other progress you have; when you have none, stop emitting
  tool calls, and the reply either drains at your next step boundary or
  revives you if you have gone quiescent (ARCH §2.11). Ending a step
  that has no work left costs nothing; a sleep costs a model call and
  learns nothing.
