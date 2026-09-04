# Design: the learning loop — a reviewer that proposes, an operator who accepts (bl-05e9)

**Status:** living document. Deliverable of bl-05e9, under the bl-8175 umbrella
(hermes lesson 3: post-response learning with staged writes). This document
numbers its own sections, so a bare `§N` is a section of *this* document and
every other reference names its document (`ARCH §N` = `docs/ARCHITECTURE.md`;
the cross-document rule, ARCH §2.1).

**Ruling being implemented (operator, 2026-09-03).** Adopt the hermes
post-response learning loop — after the user-visible work, a narrowly equipped
reviewer inspects the conversation and updates memory or skills — but with
**staged writes by default**: nothing an unattended reviewer proposes lands
without approval. The hermes reviewer's own caveat is the reason: *"the review
prompt explicitly biases itself toward finding something to save … for serious
use I would require memory/skill approval."*

**Terminology.** Three terms are coined here and recorded in `docs/TAXONOMY.md`
§3: **reviewer**, **proposal**, **workspace skill**. Everything else is ARCH
vocabulary — role, dispatch, checkpoint, compaction point, followed config
commit, work product, transcript.

---

## 1. Survey: what exists, what is missing

- **Dispatching a reviewer is config, not code.** `dispatch(<role>)` is in the
  closed action set (ARCH §6) and the compactor is the precedent for a
  narrowly equipped child forked at a step boundary whose return is consumed
  by a landing action rather than delivered into the parent's transcript
  (`land_compaction`, ARCH §2.6). PRINCIPLES *Symmetry of dispatch* names
  "verifier, and compactor" as ordinary agents; a reviewer is a third.
- **No write path exists from an agent to any skill.** Skill bodies live only
  in the install-global pool `<data-root>/skills/<name>/` (ARCH §3.3),
  populated by `litany prime` and the operator; `load_skill` copies, never
  writes. A config lineage advances only by `litany config` (ARCH §2.3).
- **No usage accounting exists**, and none is wanted: every use is already a
  commit (the `load_skill` copy lands under `skills/<name>/` on the electing
  branch, ARCH §3.3 shipped-state note), and every patch is a config commit.
- **The durable-facts document** is `docs/DESIGN_CONTEXT_ECONOMY.md`'s (in
  flight): that design owns what it is, where in the config commit it lives,
  and its cap. This document refers to it by name and defines nothing about
  it beyond how a reviewer's edit to it is staged (§4).

## 2. Decision A — the reviewer rides the compaction checkpoint

**The reviewer** is a role — `souls/reviewer.md`, a `providers.yaml` row —
dispatched by the workflow binding

```yaml
worker_flush:
  - dispatch(compactor)
  - dispatch(reviewer)
```

**Why `worker_flush` and not a reply terminal or a clock of its own.**

1. hermes's own cadence is per-iteration ("about every ten user turns",
   "roughly ten tool-loop iterations"), not per-answer; the `compaction:` clock
   *is* that cadence, already config and already derived from git (ARCH §2.7).
   A second clock would be a second block in `workflow.yaml` and a second
   "since last" derivation — new config, for a fact one clock already states.
2. **Review before you forget.** The span a checkpoint covers is the span the
   compactor is about to squash out of the transcript (TAXONOMY *compaction
   span*). The reviewer forks off the same compaction point and reads the same
   inherited transcript, so the evidence it inspects is exactly the evidence
   that is about to stop being inspectable. A reviewer at the reply terminal
   would read a span that may already have been compacted.
3. Nothing new fires. No event joins the closed set for the trigger; the
   reviewer's dispatch commit, grant gate, budget gate and confinement are the
   compactor's, keyed on the role.
4. **Never on the critical path.** The dispatch is a fork at a step boundary;
   the reviewer's return is consumed by `stage_proposal` (§3) with no transcript
   delivery, so the reviewed agent never reads a review and is never woken into
   a model call by one. The executor wakes to interpret the return, finds no
   warrant, and parks — the same wake a compactor's return costs.

**Default cadence** is therefore the compaction clock's — every 20 commits in
the basic agentic loop, which the learning-loop workflow (below) keeps. **Cost:**
one reviewer model call per compaction pass, on the reviewer row's model (the
shipped row is the compactor's, `claude-haiku-4-5`). **Off switch:** delete
`dispatch(reviewer)` from the `worker_flush` list — a config edit, not a flag;
a config with no reviewer binding never forks one.

**The hole, priced.** An agent that never reaches a checkpoint is never
reviewed. Its transcript is also never squashed, so the evidence keeps; the
operator who wants short conversations reviewed lowers `n` or binds
`trigger: on_flush`. Not filled with a second trigger, because a second trigger
needs its own "since last review" derivation.

**Shipped as a named alternative workflow, not the default.**
`docs/DESIGN_WORKFLOW_SWITCH.md` §3 pins the basic agentic loop as "exactly
today's stock behavior", and a reviewer is model spend the operator opts into.
`litany prime` seeds `<config-root>/workflows/learning-loop.yaml` beside
`basic-agentic-loop.yaml` (seed-if-absent, from an embedded asset) — the basic
loop plus the one binding above. The `reviewer` role's soul and row ship in the
template (`template/souls/reviewer.md`, `template/providers.yaml`) so every
`config/default` *can* dispatch one: a declared, unbound role is inert (ARCH
§4.3), and it is what makes switching a config edit rather than an authoring
act. Adoption: `litany config <ws> learning --from default`, replace
`workflow.yaml` with the template, then `litany workflow <ws> <agent> --config
learning` (or fork new roots off `config/learning`).

**The reviewer's confinement.** Grant `[read_file, apply_patch]` — read the
transcript and the skills, edit files, nothing else; no `bash`, no `dispatch`,
no `message`. Its manifest entry pins `goal.md`, `soul.md`; orders
`summary/**`, `skills/**` (the transcript is always composed). Its dispatch
commit **keeps the inherited dialog** (ARCH §2.2 *Branch-scoped vs inherited*
— it joins the compactor and the fork-back-in root as the third principled
keeper: the transcript is its subject) and **checks out the followed config
commit's `skills/**` and the durable-facts document** into its tree, the same
checkout the descriptor cut performs for `descriptions/**` (ARCH §3.3), so a
fresh read precedes every write by construction.

**The soul** (`template/souls/reviewer.md`) carries hermes's four look-fors —
user corrections; reusable debugging or operational techniques; a loaded skill
that failed or is outdated; a workflow worth packaging as a script, template or
reference — and its two warnings verbatim in substance: *the review prompt
biases toward finding something to save, and an empty proposal is the expected
common outcome*; and *never record an unresolved failure as a proven workflow*.
It prefers patching a broad existing workspace skill over a skill per incident,
and it states the ownership rule (§3): pool skills are not its to edit. Pinned
by an install test the way `bash`'s four environment facts are
(`src/install/tests/toolspec.rs`).

## 3. Decision B — a proposal is one commit on `proposal/<reviewer-id>`

**Workspace skills.** A **workspace skill** is a skill directory committed in
the config lineage at `skills/<name>/` — versioned, forkable, per-workspace,
and reachable by follow-the-tip. It is the only thing a reviewer may propose
against. **Ownership is the path**: a body under `<data-root>/skills/` is the
install's (shipped by `prime` or installed by the operator); a body under a
config commit's `skills/` is the workspace's. No marker file (a second home for
the fact) and no authorship derivation (an accepted proposal and a
`litany config` edit are both operator commits, so authorship cannot tell them
apart). Names are unique across the two homes: the config-authoring pass
refuses a workspace skill whose name a pool skill holds, so `load_skill`
resolves `<followed config commit>:skills/<name>/` first and the pool second
with no shadowing arm to drift. The authoring pass snapshots workspace skills
into `descriptions/skills/**` exactly as it snapshots the pool — one mechanism,
a third source. The dispatch commit trims the lineage's skill bodies from
every agent's tree as it trims the control files (ARCH §2.2): a body is not
context until elected. **The trim is an intersection, not `skills/**` whole**
(amended on implementation, bl-28e2): `skills/` is equally where an agent's
*own* elected bodies live, and ARCH §2.7 makes those the compactor's input, so
removing the directory outright would take a parent's spent skills away from
the child forked to compact them. What leaves is exactly the names the forked
tree shares with the governing config commit.

**The proposal.** A **proposal** is one config commit on the branch
`proposal/<reviewer-id>`, parented on the followed config commit the reviewer
read, whose diff is the reviewer's edits and whose message is the reviewer's
terminal response (a one-line subject the soul asks for, then the rationale).
It is minted by the workflow action **`stage_proposal`**, bound on the new
lifecycle event **`reviewer_return`** (derived from the child's dispatch-commit
role exactly as `compactor_return` is, ARCH §6):

```yaml
reviewer_return:
  - stage_proposal
```

`stage_proposal` is the reviewer's landing, the shape of `land_compaction`:

1. **Epitaph-gated.** Only a `final-response` return stages; any other epitaph
   delivers as the ordinary obituary (the general rule, ARCH §2.6).
2. **The diff is the reviewer's own commits**: founding (dispatch) commit tree →
   terminal ref tree, never `merge-base` with the dispatcher — the compactor
   the reviewer was forked beside rewrites the dispatcher's history under it
   (rebase-forward), and a merge-base would then reach back past the span.
3. **The filter admits two path classes** and nothing else: `skills/<name>/**`
   where `<name>` is a name **the install pool does not hold**, and the
   durable-facts document (§4). One path outside the classes — a loaded pool
   copy, a work product, a control file — **refuses the whole proposal**,
   naming the path: a proposal is one commit, and partial staging is a second
   shape.

   *Amended on implementation (bl-5b62), twice.* The rule used to read "a
   workspace skill **or** a new name the pool does not hold", which is two
   tests for one answer: a workspace skill's name can never be a pool name,
   because the authoring pass refuses that collision (ARCH §3.3). The pool
   query alone decides, and it decides `skills/archived/**` (§5's move) with
   no arm of its own, since `archived` is reserved in both homes. And the
   **transcript is not in the diff at all**, so it cannot be a refusal: every
   branch's executor commits `messages/**` (ARCH §2.3) and a compaction lands
   `summary/**`, so both stand in the founding→terminal range of every
   reviewer that ever spoke. They are the harness's writing, not the
   reviewer's, and step 2's diff excludes them by pathspec from the two
   modules that name them. Refusing on a transcript entry would have refused
   every proposal ever made.
4. **Fresh read-before-write, by commit identity, never by patch-applies.** The
   reviewer read the followed tip at its fork; if the lineage tip at landing
   is another commit, the proposal is refused as stale and nothing is written
   — the next checkpoint re-derives from the new tip. A config edit during a
   review costs that review, and the cost is stated rather than hidden behind
   a three-way merge.

   *Which commit was read is a mark* (bl-5b62): the reviewer's fresh read is a
   **checkout** of the followed tip into a forked tree (§2), which leaves no
   ancestry between the two commits and no other trace of the tip's identity —
   so the commit that performs the read states what it read, once, at
   `refs/litany/config-read/<reviewer-id>`. That is the established shape for
   an orthogonal, non-derivable per-agent fact (ARCH §2.2's mark namespace,
   `retarget` in particular, which likewise names a config commit and nothing
   else), and it is reaped with the agent by `litany delete`, which enumerates
   the mark root rather than a list of kinds. A return with no such mark is
   not a reviewer's landing and stages nothing.
5. **Empty diff → nothing.** No branch, no ref, no notice: the general path
   with empty inputs, and the structural answer to the bias-to-save. It is
   decided before anything is materialized, because a landing that minted
   first would land the *descriptions refresh* of step 6 as a proposal the
   reviewer never made.
6. **Minted through the config-authoring routine** (`src/template/authoring`,
   the routine `litany config` is): a transient checkout of the tip, the diff
   applied as the edit, the descriptions snapshot refreshed, every `SKILL.md`
   parsed (ARCH §3.3 bl-e3f5), teardown on every exit path, and the commit
   landing on `proposal/<reviewer-id>` instead of `config/<name>`. One path
   authors every config commit; a proposal is one that no lineage points at
   yet. **Every refusal that routine already owns is therefore a proposal's
   refusal too**, in one home and in the routine's own voice: a pool-name
   collision, a malformed `SKILL.md`, and the facts document's cap (§4). The
   routine gained one parameter for this — the branch a pass lands on — and
   the proposal origin is a fork in every other mechanical respect, including
   the teardown that deletes the ref a refused pass created.
7. **The result message is consumed**, never delivered — the reviewer's
   reasoning is on its own branch and in the proposal's message.

**One writer per branch holds.** `proposal/<reviewer-id>` is written once, by
the reviewer's dispatcher's executor at the landing, and advanced by nobody: a
re-review is a new reviewer id and a new branch. `config/*` still advances
only by the operator — acceptance is the operator verb below. Governing-lineage
and followed-tip derivations read `config/*` alone, so a proposal branch is
invisible to resolution until accepted.

**The operator verb: `litany proposal <workspace> [<id>] [--accept | --reject]`.**
One verb, modes by argument, the `litany workflow` shape:

- bare: list every `proposal/*` — id, lineage, parent, **fresh** (parent is the
  lineage's current tip) or **stale** (derived at read time, never stored),
  diffstat, subject.
- `<id>`: the proposal's message and full diff.
- `--accept`: fast-forward the lineage head to the proposal — a compare-and-swap
  `update-ref` whose expected old value is the proposal's parent — then delete
  the proposal branch. A stale proposal is **refused** with the current tip
  named; the one remedy is `--reject`, and the next checkpoint re-derives.
  Follow-the-tip (ARCH §2.2) delivers the accepted patch to every agent on the
  lineage at its next step with no act per agent.
- `--reject`: delete the proposal branch. The reviewer's own branch stays as
  the record of the reasoning and is reaped with its dispatcher like any
  child; `litany delete` reaps that agent's proposal ref beside its marks.

**Why a branch and not a message carrying a patch.** A message is text in a
transcript: acceptance would be an operator copying a patch into
`litany config` by hand, with no parent to check, no diff to show and no ref to
reject. A branch is a commit git already knows how to list, diff, fast-forward
and delete; every fact about it is a query.

## 4. Decision C — the facts document is staged the same way

A reviewer's edit to the durable-facts document is a hunk in the same proposal
as its skill edits, subject to the same filter, freshness rule and acceptance.
`docs/DESIGN_CONTEXT_ECONOMY.md` owns the document's path and cap, and a
proposal is **refused over-cap at proposal time**, naming the cap — never at
acceptance, where the operator would be handed a proposal that cannot land.

*Amended on implementation (bl-5b62).* Neither fact is read twice. The path is
the string the reviewer's own fresh read checks out (`src/prompt/dispatch/
step_commit/reviewer_read.rs`, bl-e6ed), so the class admitted *out* is the
class the reviewer was shown *in*. And the cap is **not a second check inside
`stage_proposal`**: that design already places it at
`template::authoring`'s decline, and a proposal is minted by that same routine
(§3 step 6), so the over-cap refusal reaches the proposal path by construction
— at proposal time, since that is when the pass runs — with no second home to
drift. What `stage_proposal` owns is reporting it.

## 5. Decision D — the curator is a query: `litany skills <workspace>`

A read verb, no store. Its product is the **skill census** (`docs/TAXONOMY.md`
§3): the table of every skill both homes offer, derived at read time and
stored nowhere. Per skill, one row: name; **owner** (`pool` or
`workspace`); **last use** — the newest commit on any `agents/*` branch in the
workspace that added `skills/<name>/` (the `load_skill` copy *is* the use, and
git already dates it); **last patch** — the newest `config/*` commit touching
`skills/<name>/`; and **state**:

- **active** — some living branch in the workspace has loaded it;
- **unused** — no living branch has; tool-claimed pool skills are exempt
  (they compose as tool descriptions without loading, ARCH §3.3);
- **archived** — the path is `skills/archived/<name>/` in the followed config
  commit.

**Archival is a move**, proposed the staged way (`skills/<name>/` →
`skills/archived/<name>/` in a proposal; a reviewer proposes it under look-for
three, an operator by `litany config`) and reversed by moving back. An archived
skill composes nowhere: the descriptions snapshot skips `skills/archived/**`,
and `load_skill` cannot name it, because `archived/<name>` is not a single path
component and is refused structurally (ARCH §3.3).

**`--config <name>` names the lineage, defaulting to `default`** (amended on
implementation, bl-ae06). Workspace skills and the archive container live *in*
a config commit, and a workspace may carry several lineages, so "the followed
config commit" is not a fact a workspace-scoped verb can derive — it is asked
of one lineage's tip. This is not a new knob: it is the same argument, the same
default and the same reading `litany workflow`, `litany retarget` and
`litany prompt` already give an unnamed config (ARCH §3.4). The install pool is
the box's and answers the same whichever lineage is named.

**Two exclusions make "last use" mean an election.** The walk is
`--branches=agents/*`, so a deleted agent's history leaves with its ref and
"living branch" needs no second derivation; and `--not --branches=config/*`, so
the config commit that *authored* a workspace skill is not read as an election
by every agent descended from it. What is left is exactly the adds an agent
branch made on its own. **Last patch reads both of the skill's paths** —
`skills/<name>/` and `skills/archived/<name>/` — so the archival move is itself
a patch and an archived skill still carries a date. A pool skill has no path in
the lineage at all, so its last patch is empty: the pool is the install's, and
no config commit patches it.

**Refused:** a usage counter field, a views/uses/patches store, a curator
process, and a wall-clock "stale" horizon — the last because a horizon is
policy and policy is config; the verb prints ages, oldest-used first, and a
horizon is the reader's. hermes's `stale` is therefore not a state here.

## 6. Decision E — what proves it without a model

Every piece runs on the stub-adapter harness (`src/prompt/tests/`), the
verifier gate's own proof pattern (`verifier_gate.rs`): a scripted reviewer
emits `apply_patch` tool calls, then a final response.

| Piece | Proving test |
|---|---|
| workspace skills in the lineage | authoring refuses a pool-name collision and `archived/`; snapshot carries a workspace skill's frontmatter; `load_skill` resolves the followed tip first; dispatch commit trims the lineage's bodies and keeps an elected one |
| `learning-loop.yaml`, role, soul | `workflow_vocabulary.rs` sweeps the new template; an install test pins the four look-fors and two warnings; `prime` seeds it seed-if-absent |
| checkpoint dispatches both | `advance_compaction.rs`-shaped: one due clock forks a compactor and a reviewer off one point; the reviewer keeps the dialog and carries the config's `skills/**` |
| `stage_proposal` | a scripted edit lands as `proposal/<id>` with parent == tip and the reviewer's text as message; a pool-copy edit refuses whole; an empty diff writes nothing; a `litany config` advance between fork and landing refuses stale; a non-final epitaph delivers an obituary |
| `litany proposal` | list marks fresh/stale from refs alone; accept fast-forwards and the next step's `load_skill` sees the patch; accept on stale refuses; reject deletes; command-surface parity |
| `litany skills` | a fixture workspace with a loaded, an unloaded and an archived skill yields three states and the loading commit's date |

## 7. Attacks considered

- *"The reviewer could edit its own soul or workflow."* Control files are not
  in its tree (ARCH §2.2) and the filter admits two classes; a proposal is
  never a control-file edit.
- *"A reviewer forked beside a compactor sees a tree the compactor is about to
  rewrite."* It reads a snapshot and diffs against its own founding commit
  (§3 step 2); the compactor's landing cannot move that.
- *"Two reviewers, two proposals against one tip."* Both are fresh; accepting
  one makes the other stale, and stale refuses. No merge, no ordering rule.
- *"Why not let the reviewer patch a shipped skill?"* The pool is the
  install's and is shared by every workspace on the box; a workspace's lesson
  is a workspace skill. Forking a shipped skill into the workspace under a new
  name is an operator `litany config` act.
- *"Isn't `stage_proposal` a second landing beside `land_compaction`?"* Both
  consume a child's return without delivery and mint a commit through
  existing machinery; they diverge at what they mint — a base for
  rebase-forward, a commit for an operator's fast-forward — which is exactly
  where PRINCIPLES *One obvious path* says they should.

## 8. Filed, in landing order

bl-28e2 workspace skills (§3, §5) → bl-30fe the inert reviewer role, soul,
vocabulary and `learning-loop.yaml` (§2, §3) → bl-e6ed the checkpoint forks
the reviewer (§2) → bl-5b62 `stage_proposal` (§3, §4) → bl-9a65
`litany proposal` (§3); bl-ae06 `litany skills` (§5) needs only bl-28e2. The
facts-document class of the filter is one line in bl-5b62, gated on
`docs/DESIGN_CONTEXT_ECONOMY.md` landing.

## 9. Shipped state

**bl-30fe — the reviewer ships inert.** The role exists and dispatches
nobody: `template/souls/reviewer.md` — the four look-fors and the two
warnings, pinned as literal phrases by
`src/install/tests/reviewer_role.rs` — a `providers.yaml` row granting
`[apply_patch, read_file]` on the compactor's model, and a
`manifest.yaml` entry pinning `goal.md`/`soul.md` and ordering
`summary/**`, `skills/**`. The vocabulary is real:
`Event::ReviewerReturn` parses and `Action::StageProposal` parses; the
landing behind the latter shipped with bl-5b62 (below), and the ARCH §6
*terminal-lifecycle* interpreter still declines it with
`ActionUnsupported` for the reason `land_compaction` is declined there —
both are minted from a **child's return**, and a branch's own terminal
has none (`docs/PRINCIPLES.md` "Decline illegal operations"). `litany
prime` seeds
`<config-root>/workflows/learning-loop.yaml` beside the default,
seed-if-absent.

`learning-loop.yaml` is its **own asset**, not a derivation of
`template/workflow.yaml`: composing it at seed time would either drop
that file's comments — a pool entry is copied and read by people — or
patch YAML text by hand. The second copy is held against drift by
`src/install/tests/learning_loop.rs`, which parses both and asserts that
every block but `events:` is equal and that `events:` differs by exactly
`dispatch(reviewer)` on `worker_flush` and `stage_proposal` on
`reviewer_return`. The basic agentic loop changes by zero bytes
(`docs/DESIGN_WORKFLOW_SWITCH.md` §3).

Not shipped by that ball, and each named where it belongs above: the
checkpoint that actually forks the reviewer (§2, bl-e6ed, below), the
`stage_proposal` executor (§3, bl-5b62, below), `litany proposal` (§3,
bl-9a65) and `litany skills` (§5, bl-ae06).

**bl-e6ed — the checkpoint forks the reviewer.** `worker_flush` runs
**every dispatch it binds**, off the one compaction point the clock
already chose: `child_result::flush::execute_flush` matches any
`dispatch(<role>)` in the list and `dispatch_at_point` forks it, so this
document's two bindings are two children of one commit and the basic
agentic loop's one binding is unchanged — one child, byte for byte
(`flush_clock.rs`). No second trigger, no second "since last"
derivation, no new event.

The role decides two things and nothing else. Its **checkpoint goal** —
`reviewer::reviewer_goal` (`src/prompt/reviewer.rs`, beside the role
name), short by design: which branch, what is in the tree, and the
dispatching branch's own goal quoted verbatim, because everything a
reviewer looks for is its soul's (§2, bl-30fe) and policy lives in
config. A `worker_flush` dispatch of any *other* role is **declined**
with `ActionUnsupported`: the harness has no goal to instruct it with,
and a fork with nothing to do is worse than a refusal.

And its **dispatch commit**. The reviewer joins the compactor in
`step_commit::inherited::DIALOG_KEEPERS` — ARCH §2.2's third principled
keeper, the fork-back-in root being a path rather than a role — and
`step_commit::reviewer_read::checkout` writes the followed config
commit's `skills/**` into its tree with the same `git checkout <commit>
-- <path>` the descriptor cut performs (ARCH §3.3). It runs *after* the
lineage's skill bodies are dropped from the forked tree, and the pair is
the fresh read: what the fork point carried leaves (a parent's elected
copy may be an older version of the same name), what the commit carries
arrives, so a proposal is always a patch against the commit it will be
parented on. Absent-tolerant — a lineage carrying no workspace skill
issues no git command at all.

**The in-flight suppressor stays keyed on the compactor** (ARCH §2.7,
bl-b9f0), exactly rather than by omission: a reviewer is forked at the
same boundary as its compactor sibling, so a reviewer in flight implies
a compaction in flight and the branch is already not due. The residual
is priced, not policed — a reviewer still running after its sibling's
landing does not suppress the next pass, which costs one overlapping
reviewer and is bounded by the clock the landing reset.

**Settled at bl-cb91.** The reviewer's *other* proposable class, the
facts document, is not read in here at all:
`docs/DESIGN_CONTEXT_ECONOMY.md` §3's dispatch-commit cut landed, and it
cuts `facts.md` out of the followed config commit into **every** fork's
tree — a reviewer's included. A second checkout keyed on the role would
be a redundant path to a tree state the general one already guarantees,
so `reviewer_read` now owns neither the name nor the read; the
reviewer's fresh read of the facts document is the general cut, and its
one-home name is `crate::facts::FILE`. The proposal filter's admitted
path class is unchanged.

**bl-ae06 — the curator is a query.** `litany skills <workspace>
[--config <name>]` ships: `src/skill/census.rs` derives the rows and
renders the table, `src/cmd/skills.rs` is the verb (ARCH §3.3, §3.4).
It reads only git and the install pool — three `git` invocations per
row and no state of its own — and the table always carries its headers,
so a workspace with no skills in either home prints the headers and
nothing else rather than taking an empty-case arm. Proven over a real
workspace by `src/skill/census/tests.rs`: a loaded pool skill (active,
dated by the electing commit), an unloaded workspace skill (unused,
dated by the config commit that authored it), an archived one, a
tool-claimed pool skill that is active with no election at all, the
oldest-used-first ordering, and the headers-only empty workspace;
`src/cmd/tests/skilling.rs` pins the argv shape, the product and the two
declines that precede any derivation.

**bl-5b62 — `stage_proposal`.** The reviewer's landing ships, beside the
compaction landing it is shaped after
(`src/prompt/dispatch/child_result/proposal.rs`, its filter and patch
in `proposal/edits.rs`): a `reviewer` return names `reviewer_return`, whose
baseline binding is this action for the reason `compactor_return`'s is
`land_compaction` — a review that fell through to `deliver_result` would
put itself in the reviewed agent's context, which §2 forbids. Both
landings share one epitaph gate (`landing::qualifies`), because it is one
question: did the pass this child was forked to perform finish?

Three facts got homes rather than copies. The **read mark**
(`refs/litany/config-read/<agent-id>`, §3 step 4) is written by the same
dispatch-commit step that performs the reviewer's fresh read of the
skills, since that step is the only place that knows the commit. The
**facts path** is `crate::facts::FILE`, the one home bl-cb91 gave it, so
the class admitted out is the class the fork cuts in (§4). And the
**branch a pass lands on** became a parameter of the config-authoring
routine (`template::authoring::Origin::Proposal`,
split into `authoring/origin.rs`), so a proposal is authored by the
routine `litany config` is — which is what makes the pool-name collision,
the `SKILL.md` parse and the facts cap the proposal's refusals too,
without a second check anywhere.

One dependency crossed the executor's boundary for it: `Deps` gained the
**data root**, because the filter's one question — does the install pool
already hold this name? — is a question about the pools, and the routine
judges the same collision against them.

Proved on the real-git child-result harness
(`child_result/proposal/tests/`): a reviewer's edit lands as one commit
on `proposal/<id>` parented on the tip, carrying its terminal response as
the message, with no delivery into the dispatcher's transcript; an edit
to a loaded pool copy, a moved lineage tip, a malformed `SKILL.md` and a
return with no read mark each refuse whole and leave no ref; a reviewer
whose only commit is a transcript entry stages nothing; a stopped
reviewer delivers an obituary; and a facts document over bl-cb91's cap
is refused by the routine, at proposal time. `litany proposal` (bl-9a65)
is what an operator does with what this stages.

**bl-9a65 — `litany proposal`.** The operator's half ships: the queries
and the two writes in `src/workspace/proposal/ops.rs`, the table in
`proposal/render.rs`, the verb in `src/cmd/proposal.rs` — modes by
argument, no `list` subcommand and no `--list` flag, because the
argument already says which question is being asked.

**Fresh and stale stayed derived, and acceptance is the same test.**
A row's state is the answer to "does some config head stand exactly on
this proposal's parent", asked when the row is rendered; `--accept` then
hands git that same parent as the expected old value of an
`update-ref`, so the check and the write are one atomic act rather than
a read the world can invalidate between. Two arms fall out of one
query with no case of their own: no head there is **stale**, refused
naming the tip; two heads there — a fork whose branches have not
diverged — is refused naming them, because accepting would have to
choose a lineage and choosing is the operator's.

**Three amendments on implementation.** `--accept` and `--reject`
**require an id**: a verb that acted on "the only one" would do
something different the day a second proposal was staged. A row names
its **lineages** (plural, rendered as the pool it is) rather than one
lineage, because the fork case above is real and a singular field would
have had to lie in it. And `litany delete` lists `proposal/<id>`
unconditionally beside the agent's own branch — `update-ref -d` on an
absent ref is already the postcondition, so a reviewer that proposed
nothing needs no arm.

**`src/cmd/mod.rs` gained room by a real split** (ARCH §3.4, amended):
the binding seam — `Fx`, `Outcome`, `Error` — moved to `cmd::seam`,
re-exported at `cmd::*`, and the parity checker now names `cmd`'s two
non-verb modules as a pair instead of one. The verb list and the seam
were two things at one boundary and only one of them grows with the
verbs.

The whole loop is proved end to end on the stub-adapter harness
(`src/prompt/tests/reviewer_proposal.rs`, §6's row): a scripted reviewer
emits an `apply_patch` whose worktree side effect the executor commits,
then a final response; its dispatcher's hop stages the proposal and
takes **no model call** doing it (the adapter for that hop answers
nothing at all, which is how "never on the critical path" is asserted
rather than described); an accept fast-forwards the lineage; and the
dispatching branch — untouched since before the review — elects the
skill and gets the patched body.
