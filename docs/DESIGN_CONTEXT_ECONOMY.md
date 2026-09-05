# Design: context economy — facts, history, lean compaction, context files, recovery (bl-e8ec)

**Status:** living document. Deliverable of bl-e8ec under the bl-8175
umbrella (the hermes-harness review). Amends `docs/ARCHITECTURE.md` §2.7,
§3.3, §5.5 and §6, `docs/PRINCIPLES.md` *Compaction, never compression*, and
`docs/TAXONOMY.md` §3 where a decision below changes an invariant.

**Section-reference convention.** A bare `§N` in this document names a section
of *this* document; every cross-document reference names its document —
`ARCH §N` for `docs/ARCHITECTURE.md`, anything else by path.

**Ruling being implemented (operator, 2026-09-03).** The engine is very
conservative about context: context is the scarce resource. The six lessons
of the review are borrowed where they fit litany's principles and *reframed*
where they do not — a special case is usually a missing reframe, one fact has
one home, nothing is stored that can be computed, and a new flag or verb is a
smell. Out of scope here: tool-catalog capping (`tool_search`) and the
reviewer/curator loop that *proposes* memory writes; this document owns what
the memory **is**, where it lives, its cap, and how it reaches context.

---

## 1. The shape of the answer

litany already holds most of what the review praises, as structure rather
than feature: the pinned head is frozen at dispatch (ARCH §5.5), skills are
description-always / body-on-demand (ARCH §3.3), every context-entering byte
is a committed worktree file selected by a manifest (ARCH §5.1), tool output
is a bounded projection with the full record on disk (ARCH §3.3), the
compactor is deletion-only (ARCH §2.7), and every agent is an independent git
history (ARCH §2.3). What is genuinely missing is small and named below: a
durable facts file, a search over history the model can reach, a checkpoint
trigger that reads the provider's own usage, a retained tail measured in the
unit the provider reports, a deterministic extract beside the model summary,
and directory-scoped context files carried on tool results. Everything else
in the review is either already here or refused (§9).

## 2. Terms

Each is defined once here; `docs/TAXONOMY.md` §3 carries the compaction
entries so the landing vocabulary stays in one place.

- **Facts file** — `facts.md` in a config commit: the small, capped, always
  present memory of durable facts for every agent on that lineage (§3).
- **Context file** — an `AGENTS.md` or `CLAUDE.md` discovered on the path of
  the agent's current working directory and carried on a tool result (§6).
- **History search** — the `search_history` built-in: a read over the
  workspace's `agents/*` refs that returns stored transcript entries (§4).
- **Soft archive** — the compactor's own ref, `agents/<id>-<compactor>`, which
  keeps the compaction span reachable after the landing squashes it (§5.4).
- **Window trigger** — the `window_percent` checkpoint trigger: due when the
  branch's last usage reaches a fraction of the model's context window (§5.1).
- **Last usage** — the `usage` sibling of the newest `messages/NNN-<model-id>.json`
  entry in the read-state tree: the provider's report for the last model call
  on the branch (ARCH §2.3 *Usage rides the entry*).
- **Token tail** — `keep_recent_tokens`: the retained tail (docs/TAXONOMY.md §3)
  measured in provider-reported prompt tokens rather than commits (§5.2).
- **Extract** — the deterministic compaction product: `summary/NNN.refs.md`,
  written by the landing, never by the model (§5.3).

## 3. A — Durable facts: `facts.md` on the config lineage

**Decision.** A workspace's durable facts are one file, `facts.md`, on its
config branch beside `souls/`, capped at **4096 bytes** (about a thousand
tokens at ARCH §5.2's ~4 bytes/token estimate), refused at write when over.
It reaches an agent the way `descriptions/**` does: the dispatch commit cuts
it from the followed config commit into the new branch's tree (ARCH §2.3
step 2), and the shipped `worker` manifest pins it (`pinned: [facts.md]`),
so it composes as a path-framed head block, frozen for the branch's life.
The `compactor` does not pin it. An absent file is the general path with
empty inputs: nothing composes, nothing errors.

**Where it lives, and why not elsewhere.** Four homes were weighed:

- *A facts branch.* A second lineage for one file; every agent would need a
  second governing query, and the archive (ARCH §9.2) a second ref set. No.
- *A data-root file outside git.* Not versioned, not forkable, not in the
  bundle, and — decisive — outside the tree, which ARCH §5.1 admits no
  second input from. No.
- *The system slot, composed from resolution like the soul.* Would follow the
  tip at every step (ARCH §2.2 bl-403b), so an edit lands mid-branch and
  flushes the prefix. The review's freeze is exactly against that. No.
- *A config-commit file cut into the tree at the fork.* Versioned, diffable,
  forkable, archived with its lineage, frozen at dispatch by the mechanism
  that already freezes descriptors, and manifest-selected. **Yes.**

**Who writes it.** `litany config` — the one user act that advances a config
branch (ARCH §2.2). An agent cannot write it: control resolves from the
config commit and worktree writes never reach it, so the review's "writes
do not mutate the prompt prefix mid-conversation" is structural here, not a
freeze flag. The proposing loop (reviewer → staged patch → approval) is the
learning-loop design's; its approval *is* a `litany config` commit.

**The cap is a refusal, not a shed.** `template::authoring` declines a commit
whose `facts.md` exceeds the cap, naming the size and the cap — the review's
"over-capacity writes fail explicitly rather than silently evicting". The
number is a constant of the artifact (the shape `read_file::MAX_BYTES` has),
not a manifest key: the point of a facts file is that it is small enough to
be always present, and a workspace that wants more has procedures that
belong in a skill or reference material that belongs in a work product the
agent reads on demand. Assembly's `budget_tokens` shedding never touches it
(pinned is never shed, ARCH §5.2), which is why the cap must sit at the write.

**Amended at implementation (bl-cb91): both writers of a config commit
check the cap, not one.** This section said `template::authoring`
declines an over-cap file, which is the `litany config` half. `litany
new` also writes a config commit, and its seed set is the union with
`<config-root>/template/` — the very override this section names as the
seed home two paragraphs below — so an operator's oversized
`template/facts.md` would be frozen into every new workspace's first
commit unread. `template::scaffold` runs the same
`facts::require_within_cap` after the overlay. One cap, one function,
two call sites: the ceiling belongs to the artifact, not to a verb.

**Per-user is per-workspace.** litany models no principal (ARCH §1.1:
workspaces separate concerns, not principals), so a per-user store would be
a new entity. What an operator wants from "per-user" — facts every new
workspace starts with — is already the config-root `template/` override
(ARCH §2.2): `<config-root>/template/facts.md` seeds every `litany new`, and
from then on each workspace's lineage owns its copy. No new mechanism.

**Staleness is priced, not hidden.** A long-lived root never re-reads the
file; a child re-cuts it at its fork, so a new fact reaches new agents. The
dispatch-time cut going stale under a moved tip is bl-37cd's existing
question for `descriptions/**`; `facts.md` joins that class and rides
whatever refresh it lands, at a rebuild point (ARCH §5.5), never mid-branch.

**Not the branch's history.** `facts.md` is a dispatch-written fact, so it
joins the `not_compaction_eligible` class (ARCH §2.7): a compactor nominating
it is declined at the door.

## 4. B — History search without a second store

**Decision.** No SQLite, no FTS, no index. The search surface is git over the
workspace's own `agents/*` refs, reached by one model-callable built-in,
`search_history`, whose subject is the conversation's history and which
returns **stored transcript entries verbatim**, bounded, with an address that
recovers any one of them whole.

**Why a tool and not a `bash` recipe.** In the engine's own process the
recipe is one `git log`; but the deployment litany ships into routes `bash`
to a foot on another machine (yog `docs/REMOTE.md` §5), where the workspace
repository does not exist. A tool whose subject is the conversation is an
engine act — `cd`, `load_skill`, `dispatch` are the precedent — and is the
only way the search reaches the history wherever the shell runs. The
description-always cost is one line; the body is a skill loaded on demand.

**Contract.** Input is exactly one of:

- `{"pattern": "<text>"}` — fixed-string search. The tool runs
  `git log --diff-filter=A -S<pattern> --format=%H --raw --no-abbrev <refs>
  -- messages summary` over every `agents/*` ref of the workspace (`--all`
  restricted to that namespace), newest commit first. Each hit is one
  `(commit, path)` — the commit that *added* the entry, so a squash or a
  deletion is never a hit. The result lists every hit's address on one
  line each, then the newest **five** entries' content verbatim, each capped
  at 4 KiB head + 4 KiB tail with the ARCH §3.3 marker naming its address.

  **One entry, one hit** (amended at implementation, bl-aafa). This bullet
  read *"a compactor's ref shares its parent's commits as one object walked
  once"* and derived non-duplication from it. The claim is true of the
  commits and **insufficient**: §5.4's landing does not merely share the
  live tail with the compactor's ref, it **replays** it as new commits
  (ARCH §2.6 rebase-forward). The original addition then stands on the
  archive and an identical addition on the dispatching branch, so every
  surviving entry of a compacted branch is listed twice — and doubles
  again at each later compaction. An entry's identity is its **path and
  its bytes**, not the commit carrying it, so the tool keeps the first
  (newest, hence the live branch's copy rather than the archive's) address
  per `(path, blob)` and drops the rest. `--raw --no-abbrev` rather than
  `--name-only` is what puts the blob in the listing: one parse, not one
  extra git call per hit.
- `{"entry": "<commit>:<path>"}` — the recovery path: that one entry, whole,
  subject only to the workflow's ordinary `tool_output` projection.

Neither more nor fewer inputs: a `limit`, a `scope` or a regex flag is a
knob the address already answers — narrow the pattern, or read an address.
The whole workspace is searched, not the agent's subtree: a workspace is one
concern (ARCH §2.2), each root is what the review calls a past conversation,
and cross-root recall is the lesson's point. The compactor's ref is what
makes squashed spans findable (§5.4); an agent's own `rm` before compaction
is findable on its own branch for the same reason (ARCH §5.4).

**Bounded, honest, recoverable.** Content comes from the object store
(`git show <commit>:<path>`), never from a summary; the head+tail cut is the
one bound litany already states in bytes; and the address is the recovery
path — the same shape as the tool-output marker's pointer to `output.json`,
except that this pointer is one the model can *follow* through the same tool.

## 5. C — Compaction: trigger, tail, extract, archive

### 5.1 The window trigger

**Decision.** A fourth `compaction.intermediate.trigger` variant,
`window_percent`, with `n` the percentage (the shipped template keeps
`every_n_commits`: the flip was surveyed and refused — **Surveyed
(bl-4c64)** below). It is due when the
branch's **last usage** prompt side — `input_total_tokens`, the whole
prompt with the cached slices included (ARCH §2.3: recorded, never
computed) — is at least `n`% of the model's **context
window**. ARCH §5.2 already reserved this home: "its home is a further
`compaction: trigger:` variant … one place, config rather than a second
policy vocabulary." It sits beside `every_n_commits` / `every_t_seconds` /
`on_flush`, evaluated at the same boundary by the same predicate.

**The numerator is a transcript read, not a step-record read.** Usage rides
the newest model entry (ARCH §2.3), so the state derivation reads the tree
it already holds; `steps/` stays diagnostic-only.

**The denominator is brazen's fact, delivered in-band.** litany keeps no
per-model table (ARCH §4.2, bl-35e2) and runs no `bz --list-models` of its own, so the
window must arrive on the stream it already consumes: brazen reports the
model's context window on the `Usage` event (an additive `v=1` field), and
the transcript writer records it beside the counters like any other counter
brazen adds. Filed in brazen as **bl-fb0c** (in-band on `Usage`), a sibling of
brazen bl-75f7 (the read surface). A row whose window brazen cannot state
leaves the field absent, and a workflow naming `window_percent` for such a
model is **declined at the boundary, loudly** (docs/PRINCIPLES.md, decline
illegal operations) — never a trigger that silently never fires.

**One trigger, no safety net.** The review's second threshold exists because
its first can be skipped; litany's checkpoint clock is evaluated at every
step boundary and a fired checkpoint suppresses the next until it lands
(ARCH §2.7), so there is nothing to skip. The residual — a tail that outgrows
the window while a pass is in flight — is the provider's own refusal, which
is a failed step and loud (ARCH §2.10).

**Shipped (bl-a537, amended bl-3fe6).** The variant, its `1..=100` range
check, the last-usage read (`src/prompt/compactor/checkpoint/usage.rs`)
and the unknown-window decline are in the tree, and **both numbers are
now on the wire**: brazen bl-fb0c shipped `context_window` in 0.0.9 and
brazen bl-d192 shipped `input_total_tokens` in 0.0.10, which the pin
has carried since, so the ratio this section names is two served facts divided
rather than one derived from three counters whose overlap the event
never stated. The transcript writer folds both in with no edit on either
side (the round trip is pinned against `brazen::Usage`'s own
serialization, not a transcribed key). An entry a **pre-0.0.10 `bz`**
wrote carries no total and falls back to the three-counter sum; that
reading over-states the prompt on the three dialects whose prompt
counter already contains the cached slice, so the clock fires early
rather than late — the safe direction here, unlike §5.2's tail, where
the same over-statement only shortens what is kept.

The shipped template still declares `every_n_commits: 20` — a row
brazen states no window for is *declined*, so defaulting to the window
trigger would refuse those workspaces at their first boundary. That
survey has since run and kept the default: see **Surveyed (bl-4c64)**
below, and ARCH's bl-a537 shipped-state note for the seam.

**Surveyed (bl-4c64) — the default stays commit-counted.** The survey
bl-a537 deferred ran against **brazen 0.0.9**, the pin at the time, and its verdict
is that the shipped default may not assume a window. brazen keeps no
per-model table either: it lifts the window out of the provider's own
models list, under the key a protocol or a row's `[provider.models]`
names, and reads it back off a local per-row cache. Two conditions must
both hold, and across the eight built-in rows the first holds once. **A
key must be named** — of the protocol defaults only
`google_generative_ai` names one (`inputTokenLimit`), while
`anthropic_messages`, `openai_chat`, `openai_responses` and `ollama_chat`
default to the empty key and `claude_code` is an exec row with no models
shape at all; no built-in row overrides it. So of `anthropic`, `openai`,
`mistral`, `openai-responses`, `google`, `ollama`, `claude-code` and
`openai-chatgpt`, exactly one can state a window — and the set that
cannot holds both the row a bare `bz` reaches and the exec row an
agentic workspace usually names. **The list must have been discovered** —
the data plane's own cache writer appends a bare id with no metadata, so
even `google` states nothing until `bz --list-models` has run for that
row. And the decline is not quiet: `Error::CompactionWindowUnknown` lands
at the first boundary after the first model entry and at every boundary
after it, so a flipped default would let such a workspace take one step
and no more. `template/workflow.yaml` therefore keeps `every_n_commits:
20`, `keep_recent_tokens` stays unset beside it (§5.2), and the variant
stays an opt-in two-line edit for a row that does state a window. What
would reverse it is a change in **brazen**, not here — built-in rows
naming a `context_key` their provider serves, or a window that does not
wait on a discovery call. ARCH's bl-4c64 shipped-state note carries the
row-by-row reading and the quoted decline.

**A window an operator sets is still not a window brazen states
(bl-3fe6).** brazen bl-f19d, which reached the pin at 0.0.10, made a provider row's
`body_defaults` `extra` fold one namespace deep, so an `ollama` row's
`options.num_ctx` now reaches the request instead of being dropped
beside the typed caps. That changes the window the model actually runs
with; it does **not** put a `context_window` on the `Usage` event, which
is lifted from the models list under a key `ollama_chat` does not name.
So the number in force and this trigger's denominator can now be made
the same fact by one operator — and until brazen states it, that
operator's row is still declined. The survey's verdict is unmoved, and
what would move it is unchanged: a change in brazen's rows.

### 5.2 The token tail

**Decision.** `keep_recent_tokens: <n>` beside `keep_recent`; declaring
both is declined at parse. The compaction point is the **oldest**
model-entry commit that leaves the stretch above it costing at most `n`
prompt tokens to append — the retained tail is the longest such stretch,
in the provider's own count, no tokenizer. The
point is always a step boundary because the count only exists at one; the
review's "10K–25K recent tail" is this number. `keep_recent` stays for the
commit-count case and for `every_n_commits`, whose `n` it must stay below.

**Amended at implementation (bl-bc20), twice.** The point was first
written here as "the *newest* model-entry commit whose last usage is at
most `n` below the tip's", which is the tip itself — every commit
qualifies and the newest of them is the tip, so the tail would always be
empty. The rule the sentence was reaching for is the one above: walk
newest-first and take the last candidate still inside the budget. And the
shipped value moves out of this decision: `template/workflow.yaml` leaves
`keep_recent_tokens` **unset**, because a token tail under a commit clock
leaves a stretch where the clock is over threshold and the span keeps
coming back empty — the branch re-walks its transcript at every boundary
and compacts nothing until the tail outgrows the budget. That is the shape
`keep_recent >= n` is refused for, in units no load-time check can
compare. The token tail belongs with the window trigger (§5.1), and both
flip together under the same survey of provider rows — which ran under
bl-4c64 and refused the flip, so both stay as shipped.

### 5.3 The extract: a second compaction product, code-written

**Decision.** At the landing, code derives an **extract** from what the
compaction removes from context — the transcript entries present at the
compaction point and absent from the base — and adds `summary/NNN.refs.md`
to the base beside the compactor's `summary/NNN.md`. Its sections, in fill
order under one byte cap: verbatim user messages (`messages/NNN-user.md`
bodies, the review's "verbatim user messages"), error strings (the last
lines of `is_error` tool results and non-zero exit tails), pull-request
numbers, commit shas (7–40 hex), and paths — each section deduplicated,
newest first, the cap stated in `compaction.intermediate.extract_bytes`
(shipped: 32768; omit it and no extract is written — severable like
`tool_output`). The name sorts after its summary (`003.md` < `003.refs.md`),
so the model reads the prose first and the list second, and
`drop_oldest_summaries` sheds the pair together.

**Reconciled with "Compaction, never compression".** The principle bounds
what the *model* may write: a deletion-only toolset so the worst case is
lost, never corrupted, information. The extract widens no toolset — the
compactor still has its pair — and is written by the landing exactly as the
landing already writes the base commit: a pure function of git, replayable,
correct by construction where a model's prose is correct by judgement. It is
therefore a **compaction product** (docs/TAXONOMY.md §3, amended): the
landing carries the deletions, the summary, and the extract, and nothing
else. It is not a second representation of a stored fact: the span it reads
is leaving context, and the extract is the fact's only remaining home *in
context*; its full source stays on the soft archive (§5.4), where §4 reaches it.

**Bytes, not tokens, and one cap.** The extract is a file in the tree, so the
bound is stated in the unit the tree has. A single cap with a fill order is
one policy; per-section caps would be five.

**Three ways it writes nothing, all one rule** (bl-e655, the implementation).
No `extract_bytes`, no summary this pass, no reference in what was removed:
each is the general path with empty inputs, not a case. The second is the
"beside" in the decision above — the extract annotates a summary and sheds
with it, so a deletions-only pass leaves no `NNN` for it to take. The third
is what the cap is spent on: an extract in which not one reference fits is
not a truncated extract but no extract, because a file saying only that
everything was omitted tells the model nothing it can act on.

**The cap bounds the references, not the file.** The extract's opening
frame and its truncation marker are structure — the same rule the tool-output
envelope states (ARCH §3.3, where the header and the `--- stderr ---` marker
are never cappable) — so the marker can always say what it cut. Everything
between them is `extract_bytes`.

**Its not-eligibility is structural, not a fourth predicate.** ARCH §2.7 puts
*this pass's* extract in the not-compaction-eligible class beside this pass's
summary, and it is there by construction: the landing derives and stages the
extract into the **dispatching** branch's base commit, so it never exists on
the compactor's branch for the pass to nominate. An earlier pass's extract
does exist in the inherited tree and is nominable, exactly as its summary is.
The same construction keeps the summary numbering honest: `write_summary`
scans for stems that parse as integers, and `003.refs` does not.

### 5.4 The soft archive is already there

**Decision.** Nothing new. The compactor forks off the compaction point with
the branch's whole history (ARCH §2.3 *Fork and inheritance*, §2.7), its ref
is `agents/<id>-<compactor>` — under the agent's own id namespace — and the
landing leaves it standing: "the squashed commits stay reachable from the
compactor's own ref until it is retired" (`compactor::land`). Retention is
reachability (ARCH §9.2): `litany delete <id> --children` reaps the archive
with the agent, `litany bundle` carries it, and nothing expires on a timer.
That ref is what §4 searches. Stated here so the property is named and
tested rather than incidental: a test pins that a pre-compaction entry is
findable by `search_history` after the landing.

## 6. D — Context files on the tool result

**Decision.** A tool result carries, appended after its envelope, every
**context file** on the path of the agent's current working directory that
this agent has not yet been shown. The names are workflow policy,
`context_files: [AGENTS.md, CLAUDE.md]` in `workflow.yaml` (shipped;
omit the block and nothing is discovered). The path is from the enclosing
git repository's top level down to the cwd when the cwd is inside one, else
the cwd alone. Each file is bounded by the `tool_output` head/tail policy as
its own stream and framed `<file path="…">` like every other file the model
is shown (ARCH §5.3). The pinned head never changes; the transcript tail
grows by an append, which is the one cache-safe direction (ARCH §5.5).

**Not a `cd` side channel — a query at every tool result.** Hanging the
discovery on `cd` alone misses the cwd's *other* writer, the `--cwd` seed at
creation (ARCH §3.3), which has no tool result to append to. The general
rule dissolves both: the fact is the cwd, the carrier is whichever tool
result comes next, and `cd` is just the tool whose result usually comes next.

**A settlement appends nothing.** A tool window that ends without
answers commits one in-band `is_error` result per unanswered `tool_use`
(ARCH §2.9, §6) — no envelope, no exit code, because nothing ran and
none is invented. The append is defined against that envelope, so a
settlement has nothing to append after; and since "shown" is derived
rather than marked, the files stay due and ride the next result an
execution actually produced. Landed by bl-b66b.

**Shown once per agent, derived.** "Already shown" is a query over the
transcript in the read-state tree — a tool entry framing that path — so no
mark is written and nothing can drift. After a compaction removes those
entries the file is shown again, which is right: the model lost it.

**Where it cannot reach.** The engine stats the cwd; when a deployment
routes tools to a foot on another machine, the cwd is that machine's and
the engine appends nothing. That foot may carry its own discovery; it is not
litany's to do and is noted, not solved.

## 7. E — Oversized tool results: met, with two gaps named

The bounded projection (ARCH §3.3, bl-d5fa) *is* the lesson: head and tail
kept, the middle cut, the marker stating counts and where the full record
lives, nothing lost. Two gaps, neither this document's to close:

- `read_file` refuses above 1 MiB instead of projecting, and has no
  offset/limit, so a partial read of a large file is a `bash` recipe. That is
  a tool-corpus contract (the tool-injection owner's), not a projection gap.
- The marker's recovery path is `steps/…/output.json`, outside the worktree:
  reachable by `bash` on the engine's box, unreachable from a foot. §4's
  address is the model-followable pointer for *transcript* content; the
  raw capture stays diagnostic (ARCH §2.3), and "re-run with a filter" —
  the marker's own advice — remains the answer for it.

## 8. F — Recoverability: what git gives, and the verb refused

**Already given.** Independent histories: one branch per agent, one writer
(ARCH §2.3). Fork-from-history: any commit is a legal fork point, so
"put the agent back at step N" is a new agent off that commit (ARCH §2.3,
§3.5). Soft archives: an `rm` is recoverable until compaction, and the
compaction span stays on the compactor's ref (§5.4). Whole-run archives:
`litany bundle` / `replay` / `delete` (ARCH §9.2). Checkpoints: every step's
read state is a real commit (docs/PRINCIPLES.md).

**Refused: a rollback verb that rewinds a branch and keeps later edits.** It
would be a second writer on an agent branch (the one-writer invariant), and
its landing would be a cherry-pick — the merge ARCH §2.6 deleted from the
system. Both halves of what it wants already exist as primitives: rolling
*context* back while keeping work products is the agent deleting the
transcript entries after N (docs/PRINCIPLES.md *Agents manage their own
context by `rm`*); rolling *files* back while keeping the conversation is
`git checkout <sha> -- <paths>` in the agent's own `bash`, committed as a
tool side effect like any other. The operator's act is a message asking for
either, or a fork.

## 9. What this refuses, and the principle it collided with

| Wanted by the review | Refused as | Because |
|---|---|---|
| SQLite FTS5 over messages | a second store | Single source of truth; `git log -S` over the refs is the index |
| per-user memory | a principal | ARCH §1.1; the `template/` seed is the cross-workspace half |
| a facts cap in config | a knob | the cap is the artifact's shape; more facts is a skill or a file |
| an 85% safety-net trigger | a special case | the clock cannot skip a boundary; one trigger |
| a third compactor tool for the extract | a wider write surface | deletion-only stays structural; the landing writes it |
| `cd`-only context discovery | a side channel | the cwd has two writers; every tool result is the carrier |
| a rollback-with-later-edits verb | a second writer + a merge | ARCH §2.3, §2.6; `rm` and `git checkout --` already do it |
| token counts anywhere litany would compute them | a fabrication | only provider-reported counts are used; bytes elsewhere |

## 10. Implementation balls and document amendments

Filed under bl-8175, each gated on this document (`--needs bl-e8ec`):

- **bl-cb91** — §3 facts file: the fork cut, the manifest pin, the write cap,
  the eligibility class.
- **bl-aafa** — §4 `search_history` built-in, schema and skill; the §5.4
  archive test rides it.
- **bl-a537** — §5.1 `window_percent` trigger (needs brazen bl-fb0c for the
  denominator; the variant and its decline land first).
- **bl-bc20** — §5.2 `keep_recent_tokens`.
- **bl-e655** — §5.3 the extract at the landing.
- **bl-b66b** — §6 context files on tool results.

Amended by this ball: ARCH §2.7 (eligibility class, compaction product),
§3.3 (context files), §5.5 (pinned head names `facts.md`), §6 (the
`compaction:` block, `context_files:`); docs/PRINCIPLES.md *Compaction,
never compression* (the extract); docs/TAXONOMY.md §3 (compaction product,
soft archive, token tail, extract).
