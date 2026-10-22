# Design: procedures — the workflow's own children, general (bl-06ae)

**Status:** living document. Deliverable of bl-041b under the bl-06ae epic;
each numbered section below is implemented by one subtask of that epic, and
a section's shipped-state note is written by the ball that ships it. This
document numbers its own sections, so a bare `§N` is a section of *this*
document and every other reference names its document (`ARCH §N` =
`docs/ARCHITECTURE.md`; the cross-document rule, ARCH §2.1).

**Ruling being implemented (operator, 2026-10-21).** The engine must run
**procedures** beside a conversation: a child the workflow forks, out of
band, whose context never enters the parent's and whose *conclusion* alone
lands. A titler on a cheap model. An independent evaluation of a proposed
change. A watcher for injected instructions. In the operator's words: the
branch should exist, but only the conclusions merge. Three further rulings
bound the design. **No graph** — the operator relitigated the DAG and
withdrew it, ARCH §6 already holding that a hop has no position; decision
points are events. **No builder** — a rendering of what will happen on
enter is wanted, and it is a seat's (yog bl-20d2), never an authoring
surface. **Role-generic verdicts, on one condition**: nothing can approve
anything. §5 states the guards.

**Terminology.** No term is coined. *Procedure* is ARCH §6's ("subordinate
routines invoked by the workflow … are procedures"); a **procedure child**
is an agent such a routine forks (ARCH §2.6 already uses the phrase);
**machinery** is `src/prompt/procedure.rs`'s word for the same thing seen
from the compaction clock. *Landing*, *verdict*, *epitaph*, *workflow mark*
and *role* are ARCH vocabulary. `land_response` is an action name and
`titler` a role name, neither a term.

---

## 1. Survey — what exists, what the three examples each lack

ARCH §6 already holds most of the ruling as config:

- **A child whose return never enters the parent's transcript** is the
  compactor and the reviewer: `dispatch(role)` at `worker_flush`, a return
  consumed by a **landing** (`land_compaction`, `stage_proposal`) instead
  of delivered. The parent's context is untouched because a landing is a
  commit, never a transcript entry.
- **An independent evaluation** is the verifier gate (ARCH §6
  *gate_return_on*): a child forked off the worker's terminal ref, whose
  leading `APPROVE` line lifts a hold on the worker's result. Shipped,
  config-only.
- **A per-invocation adjudicator** is `tool_control:` (ARCH §3.3), and a
  watcher over what came *back* is an `on_tool_return` hook.
- **Switching workflow mid-conversation** is the workflow mark
  (`docs/DESIGN_WORKFLOW_SWITCH.md`).
- **`deliver_result` carries only the work-product diff and the terminal
  response** (ARCH §2.6). A child's transcript stays on the child's branch.
  The fear that a child's context lands on its parent is unfounded today;
  what is missing is a landing that carries *neither* diff nor message.

Four gaps, each a small subtraction or generalization rather than a feature:

1. **`dispatch` in a per-step hook is declined**
   (`src/prompt/workflow_actions.rs`: "Any other closed-set action in a
   hook (e.g. `dispatch`) is declined"), a tracked follow-on of bl-bd8a.
   Every procedure the ruling names is a dispatch at a hook or at a
   message.
2. **The harness-instructable role set is a hard-coded `match`**
   (`procedure.rs::checkpoint_goal`: compactor, reviewer). That match is
   also the compaction-exclusion set — *machinery is never the subject of
   machinery* (bl-08b4) — and a general hook dispatch recurs bl-08b4's
   infinite fork unless the set generalizes with it (§2).
3. **`user_message` has never fired.** It is in the event set, bound to
   `dispatch(worker)` in every shipped config and every workspace's frozen
   `config/default`, and no hop derives it. A dead binding (§3).
4. **Every landing is bespoke** — one executor per consumer — and the
   verdict partition is wired to one role name (§4, §5).

## 2. Dispatch from a hook, and the machinery set derived from the workflow

**`dispatch(<role>)` executes at `pre_step`, `post_step` and
`on_tool_return`** through the same door `worker_flush` already spends
(`child_result::flush::dispatch_at_point`, i.e. `child_dispatch::run_procedure`):
a child forked off the **tip the hook observed** — for `post_step` that
includes the model's committed output, for `on_tool_return` the committed
tool results — wearing the role's soul, model and grant from the governing
config commit, with its id `<parent>-<sub>` so its return lands in the
dispatcher's inbox like every other child's. The hook executor therefore
needs what the flush executor has (`workspace` and `Deps`), and
`run_step_hook` grows to take them; nothing else about the hook seam moves.

**The goal.** A procedure child is instructed by the harness, not by a
model, so its goal is composed, not written. Today two builders exist and
both quote the dispatching branch's own pinned `goal.md`
(`compactor_goal`, `reviewer_goal`); they stay, because their prose names
the procedure's injected tools and the compaction point, which a soul
cannot know. Every other role gets the **generic goal**: *you are the
`<role>` for branch `<id>`; its goal follows*, the branch's `goal.md`
quoted. What the role *does* with that is its soul's (`souls/<role>.md`),
which is where a role's instruction already lives (ARCH §4.3). One home
stays `procedure.rs`: the role-owned builders first, the generic one as the
fallthrough — the special case dissolves into the general path rather than
being refused.

**The machinery set is derived from the workflow, not enumerated in code.**
The **self-firing events** are the ones a branch's own hop fires with no
child returning and no model asking: `user_message`, `pre_step`,
`post_step`, `on_tool_return`, `worker_flush`. A role named by a `dispatch`
bound on any of them — baseline defaults included, so the compactor stays
machinery when `worker_flush` is unbound — is machinery under that
workflow. Two readers, one derivation (`Workflow::machinery()` replaces
`procedure::is_checkpoint_child`):

- the compaction clock excludes it (bl-08b4's invariant, unchanged in
  meaning, widened in scope);
- **a branch wearing a machinery role runs no self-firing binding** — no
  hook, no flush, no `user_message`. This is the base case: a titler forked
  at `post_step` inherits the transcript and the workflow, and without this
  rule its own `post_step` forks a titler, which forks a titler. Stated
  once, of the set, rather than per role as bl-08b4 had to.

The `budgets.max_depth` ceiling (ARCH §6) remains the backstop, as it is
for every dispatch. **No in-flight suppressor** is added for hook
dispatches: a procedure's cost is bounded by the cadence of the event that
forks it, which is config, and the compaction clock's suppressor already
exists for the one event with a clock behind it. Priced, not policed.

## 3. `user_message` fires — for configs at version 2

**The circumstance.** A hop whose drain delivered at least one message from
the operator (`inbox::USER_SENDER`, ARCH §2.3 *Origins*) fires
`user_message` once, after delivery and before the step — the same seam the
delivered-child-result interpretation runs at. Once per hop, not per
message: the event is "an operator spoke", a circumstance, not a count. A
root's **first** prompt takes the same path — `run_exchange` deposits it
and the drain delivers it as `messages/001-user.md` — so the general path
covers the conversation's opening turn without a bootstrap arm. A child
whose inbox receives an operator message is a taken-over child (ARCH §2.6
*user is nobody's inbox*), and the event fires there too, which is what
taking over means.

**Why it is version-gated.** Every workspace's frozen `config/default`
carries `user_message: [dispatch(worker)]`, which has never run. Under §2
that binding would now fork a procedure worker on every prompt of every
existing conversation. Declining it at parse would refuse every existing
workspace at its next step; silently skipping it is the one thing
`docs/PRINCIPLES.md` forbids. So it is what ARCH §10 exists for:
`config::version::SUPPORTED` becomes **2** and **a version-1 config's
`user_message` bindings stay unfired** — the behaviour they have always
had, now stated as that version's rule rather than as an accident. The
template's `version` file says `2` and the dead binding goes, from the
template, the seeded alternative and the experiment variant alike, so no
shipped config ever carries a live `user_message: [dispatch(worker)]`;
the titler's binding (§6) is the first live one. Version 1 stays
readable; a version-2 template means `litany new` authors version-2
workspaces. **The machinery derivation is version-aware for the same
reason** (`procedure::self_firing`): under a version that never fires
`user_message`, a role bound there forks nothing and is not machinery —
otherwise every version-1 root, a `worker`, would read as machinery and
stop compacting. The one
reading of the version is `resolve_worker`'s (ARCH §10), which today parses
and discards it; it now rides `WorkerConfig` to the firing site.

The ARCH §6 example block still shows `user_message: [dispatch(worker)]`
as its first line; that line is amended to the titler binding (§6) when the
template changes, so the example and the shipped file stay one text.

> **Shipped-state note (bl-7192).** §2 and §3 are shipped as described,
> with one derivation beyond §2's two readers: the inherited-dialog
> keeper rule (ARCH §2.2) was a third role list, `DIALOG_KEEPERS`, and
> it is now the same machinery derivation, passed down the fork as
> `SpawnRequest::keeps_dialog` by `child_dispatch::run`, which already
> reads the governing workflow for the budget gate. `run_step_hook` takes
> the branch's own role and the `Deps`; `Delivery::operator_spoke` is the
> circumstance; `Version::fires_user_message` is the gate, asked on both
> drivers at the delivered-child-result seam. Proved over a real
> workspace in `src/prompt/tests/procedures.rs`.

## 4. `land_response(<file>)` — the generic landing

**The action.** `land_response(<file>)` takes one positional argument, a
flat name at the branch root (the action grammar admits no `/`), and
writes the returning child's **terminal response** to that file on the
dispatcher's branch as one commit, subject `landing: <file> [<child-id>]`,
then consumes the result message. **No work-product transfer, no transcript
entry.** The commit is the record and git history is the file's log; a
second landing overwrites, and "what did it say before" is `git log` on
the path, never a second file.

**Epitaph-gated exactly as `land_compaction` and `stage_proposal` are**
(`landing::qualifies`): only a `final-response` return lands; a `stopped`,
`budget-exhausted` or `died` return delivers as the ordinary obituary,
because a procedure that did not finish has no conclusion to land and its
ending is what the dispatcher should see.

**Out of band in both directions.** The file is not assembled into the
dispatcher's model context unless its manifest pins or orders it (ARCH
§5.2), and the shipped manifest names no such file — so a title lands on
the branch without costing the conversation a token, and a seat reads it
with `git show agents/<id>:<file>`, the same read it makes for `name`.

**Reserved names are refused at parse.** The harness's own control files
and directories at the branch root — `goal.md`, `soul.md`, `name`,
`facts.md`, `summary`, `messages`, `skills`, `descriptions`, `steps`,
`tools` — may not be a landing target; `land_response(name)` would be a
second writer of a fact that is immutable by rule (ARCH §2.1).

**Three landings become one shape.** `land_compaction` and `stage_proposal`
remain their own executors — a rebase-forward and a config-lineage commit
are not a file write — but all three share the qualification gate and the
consume-without-delivery discipline, and `land_response` is the general
member. It belongs to the delivered-child-result seam like the other two,
so the terminal-lifecycle executor declines it with `ActionUnsupported`:
there is no return at a branch's own terminal.

> **Shipped-state note (bl-f459).** Shipped as described:
> `Action::LandResponse { file }` (`config::action`, the reserved set
> refused at parse), executed by `child_result::landing::land_response`
> under the same `qualifies` gate as the two bespoke landings, and
> declined at the terminal-lifecycle and self-firing seams. Nothing
> shipped binds it yet; the titler (§6) is its first consumer.

## 5. Role-generic verdict events, and the gate over them

**The event set opens along one axis.** Today's `worker_return`,
`compactor_return`, `reviewer_return`, `verifier_approve` and
`verifier_reject` are five spellings of two shapes:

- **`<role>_return`** — a child of `<role>` returned. `return` is a
  reserved suffix; `worker_return`, `compactor_return`, `reviewer_return`
  are this shape and keep their baseline defaults (`deliver_result`,
  `land_compaction`, `stage_proposal`).
- **`<role>_<token>`** — a child of `<role>` returned on `final-response`
  with a **leading line whose first word is `<token>`** (case-insensitive),
  *and the workflow binds that event*. A token nobody bound is no event:
  the return is `<role>_return`. The split is at the last `_`, so a role
  name may carry underscores and a token may not.

**The derivation, in order, for a returning child of role R whose leading
word is T** (R read from the dispatch commit subject, ARCH §2.5):

1. **Gating child.** If R's return is the gating child of a held result
   (ancestry, `find_gated_worker` generalized), the gate's predicate
   `gate_return_on(R.P)` decides: epitaph `final-response` and T = P ⇒
   `R_P` (met); anything else ⇒ **`R_reject`** (unmet). `reject` is the
   second reserved suffix: it names the gate's unmet outcome, not a word
   the child said — which is what `verifier_reject` has always meant
   ("every other finished outcome rejects"). Met runs the `R_P` bindings
   (baseline `deliver_result`, which drains the held result); unmet runs
   the `R_reject` bindings (baseline `dispatch(worker, with: R.feedback)`,
   the re-dispatch).
2. **Bound token.** Else if epitaph is `final-response` and `R_T` is bound:
   `R_T`.
3. **Return.** Else `R_return`.

**The gate generalizes with it.** `dispatch(R)` bound on any `<x>_return`
event forks R off the **returning child's terminal ref** with the generic
goal quoting that child's response (today's `verifier::dispatch`, for any
R), idempotent by ancestry (today's `already_gated`, for any R).
`gate_return_on(R.P)` stays the no-op marker it is: its one reading is the
derivation above, and its presence beside the dispatch is what leaves the
result in the inbox. `verifier_approve` / `verifier_reject` parse unchanged
and mean exactly what they did; the verifier is the general path with a
particular name.

**The condition: nothing can approve anything.** Four guards, none new:

- **The role is the harness's fact.** It is read from the dispatch commit
  subject the harness wrote at the fork, never from the child's text; a
  worker that writes `APPROVE` fires `worker_approve`, which nothing binds,
  so it is `worker_return` and delivers as the work it is.
- **A verdict is read only in the dispatcher's inbox.** `own_result_ref`
  already admits a result message as a lifecycle circumstance only when
  its sender is this agent's own child; a foreign agent's "verdict" is an
  ordinary message.
- **The vocabulary is declared.** Only tokens the workflow binds for that
  role exist; the set is config, read from the governing commit, and an
  unbound token is silence.
- **A gating child is found by ancestry**, forked off the very result it
  judges; a verifier the model dispatched by hand gates nothing.

What the guards do not cover, stated rather than implied: a verifier whose
*own* context was poisoned can approve. That is the model's trust boundary,
bounded by the role's `tools:` grant (a read-only verifier cannot be made
to act) and by the watcher §7 composes — not by this design.

> **Shipped-state note (bl-c0c6).** Shipped as the three-step derivation
> above (`child_result::circumstance`), with `Event` opened along the one
> axis (`config::event`: the closed six, `Return(role)`,
> `Verdict { role, token }`, `Reject(role)`; keys split at the last `_`,
> tokens lowercase, closed names winning over the shape) and the gate
> generalized over any role (`verifier.rs`: `dispatch(<role>)` at a return
> forks off the returning child's ref, `already_gated` and the held-result
> lookup are by ancestry for any role, the unmet baseline re-dispatches the
> *gated* role). The verifier keeps its one role-owned goal text; every
> other gating role gets the generic procedure goal plus the returning
> response. Proved in `child_result/tests/verdicts.rs`, the four guards
> each as a test.

## 6. The titler — the first consumer

A **`titler`** role ships in the template, declared and *bound* — unlike the
reviewer, which ships unbound, because titling is the ruling's named want:

- `souls/titler.md`: answer with one line of at most six words naming what
  the conversation is about; no trailing punctuation, no quotes, nothing
  else.
- `providers.yaml`: `titler` on the compactor's model (haiku-class), no
  `tools:` — it reads, it answers, it cannot act.
- `manifest.yaml`: pinned `goal.md`, `soul.md`; order `summary/**`; the
  compactor's budget and overflow, since its subject is the same history.
- `workflow.yaml`, in the basic agentic loop:

  ```yaml
  user_message:
    - dispatch(titler)
  titler_return:
    - land_response(title)
  ```

So: the operator speaks → the hop delivers it and fires `user_message` →
a titler forks off the tip, inheriting the transcript up to and including
that message → it answers one line → its return fires `titler_return` →
the line lands at `agents/<root>:title` → the seat's ladder reads it (yog
bl-e6ad: `title` above the stored `name`, nothing on the wire changes). The
titler is machinery under §2 (named at a self-firing event), so it fires no
hook and no clock of its own, and it runs no `user_message` because no
operator speaks to it. One cheap call per operator turn, a title that
follows the conversation, and no token of it in the root's context.

> **Shipped-state note (bl-8c94).** Shipped as described, in all three
> shipped workflows (the basic loop, the seeded learning loop and the
> experiment variant, which the install tests hold to differ from the
> basic loop in exactly their own bindings). `src/install/tests/titler_role.rs`
> pins the role; `src/prompt/tests/procedures/user_message.rs` drives the
> shipped template end to end over a real workspace: an operator message
> forks the titler, and its return lands `title`.

## 7. What this does not solve

- **The switch as an action.** A classifier role's verdict (`classifier_<token>`)
  can be bound to anything in the closed set — but the closed set has no
  action that writes the workflow mark, so "this verdict moves the
  conversation onto workflow X" is a verb today (`litany workflow`), not a
  binding. Adding `switch_workflow(<config>)` is one action, one executor,
  and composes with everything above. **Held for an operator ruling**: the
  operator asked what a mark is before deciding, and this design does not
  pre-empt the answer.
- **The injection watcher.** It is config over this design, not a
  mechanism of its own: `on_tool_return: [dispatch(guard)]` with a guard
  soul that answers `CLEAN` or `INJECTED` and `guard_injected:
  [land_response(guard)]` — the token is bound, the guard is machinery,
  the landing is the generic one. The *natural* landing is a mark
  (`notify_ui`: the operator should look), and the verdict event fires
  for it today, but `child_result::execute_child` still declines a ref
  mark at a child's return; running the two marks there is one tracked
  executor change, filed as the follow-on that also closes this sentence.
  Nothing else is built for the watcher, and nothing else needs to be.
- **The rendering.** What will happen on enter is the governing workflow's
  bindings over the next hop's circumstances; a seat shows it (yog
  bl-20d2). It is read-only by ruling, and this document is the vocabulary
  it renders.
- **Migration of frozen version-1 configs.** They keep their behaviour. A
  workspace that wants titling re-authors its lineage at version 2; whether
  `litany config` should offer that as one act is not decided here.
