+++
title = "Workflow-run procedures become general: dispatch from a hook, one landing for a child's conclusion, role-generic verdict events, and titling as the first consumer"
created = 1792631675
updated = 1792631676
priority = 2
root_commit = "12899370c9ec7a5ed7f8e26d3d4fb914ea6c3310"

[[blockers]]
id = "bl-041b"
on = "close"

[[blockers]]
id = "bl-7192"
on = "close"
+++
## Ruling (operator, 2026-10-21)

The engine must be able to run **procedures** beside a conversation — a child the workflow forks, out of band, whose context never enters the parent's and whose *conclusion* alone lands: a titler on a cheap model, an independent eval of a proposed change, a watcher for injected instructions. The operator's words: the branch should exist, but only the conclusions merge. Three further rulings:

- No graph. The operator relitigated the DAG and withdrew it: ARCH §6 already holds that a hop has no position — it derives its circumstance from disk and runs what matches. Decision points are events. No builder either; a *rendering* of what will happen on enter is wanted (separate, seat-side ball).
- A classification that moves the conversation onto another workflow is a verdict event bound to a switch; the switch verb exists (`litany workflow`, the workflow mark). Whether it becomes an action awaits a ruling — the operator asked what a mark is before deciding.
- Role-generic verdicts are agreed on the condition that nothing can approve anything: a verdict is read only from a child the harness dispatched, by the role in its dispatch commit, in its dispatcher's inbox, against tokens the workflow declares for that role.

## Survey — what exists

- Event→action bindings, the closed action set, `dispatch(role)` from `worker_flush`, landings that consume a return without delivery (`land_compaction`, `stage_proposal`), the verifier gate, tool control, the workflow mark (ARCH §6).
- `deliver_result` already carries only the work-product diff and the terminal response; a child's transcript never reaches its parent. The fear that it does is unfounded today; what is missing is a landing that carries *neither* and writes the response to a named file.
- Per-step hooks fire but `dispatch` in one is declined (`src/prompt/workflow_actions.rs`, a tracked follow-on of bl-bd8a).
- `user_message` is in the event set, bound to `dispatch(worker)` in every shipped config, and **never fired** — no hop derives it. A dead binding in every workspace's `config/default`.
- The harness-instructable role set is a hard-coded match (`src/prompt/procedure.rs`), which is also the compaction-exclusion set ("machinery is never the subject of machinery", bl-08b4). A general dispatch-from-hook recurs that hazard unless the set generalizes with it.

## Deliverables (subtasks)

1. Design doc `docs/DESIGN_PROCEDURES.md` — lands first; every later subtask implements a numbered section of it.
2. Dispatch from a hook, the machinery set derived from the workflow, and `user_message` fired at operator-message delivery (version-gated: a v1 config's `user_message` bindings stay unfired, as they always were).
3. `land_response(<file>)`: the generic landing.
4. Role-generic verdict events and the gate generalized over them.
5. The titler: role, soul, provider row, manifest entry, bound in the basic agentic loop on `user_message` → `titler_return: [land_response(title)]`.

Seat side (other repos): yog reads `agents/<id>:title` as the top rung of the display ladder; nothing on the wire changes because the ladder's answer is what crosses.