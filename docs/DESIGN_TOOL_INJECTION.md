# Design: host tool injection at the binding (bl-9001, inverted bl-a00a)

**Ruling: one optional object on `cmd::Fx`, carrying both halves — extra
tool definitions and an execution router — and nothing else moves.** A
linked binding may put tools of its own in front of the model and answer
them itself; the pool, the grant model, the tool control, the result
envelope, the disk record and the prompt-cache discipline are all
untouched. This document records the seam, defines its terms, and states
what it is contained by — in particular why it does not reopen the
`docs/DESIGN_MCP_BRIDGE.md` refusals.

**Amendment (bl-a00a): the router's scope is total, and only its scope
changed.** The seam as first landed was consulted *ahead of* the ARCH
§3.3 three-hop binary resolution and could decline a name, which fell
through to a local spawn. It no longer can: while an injection is
installed it answers **every** invocation the agent makes, and nothing
resolves a binary behind it. The object is the same object, both halves
are the same halves, and an injected name still outranks an elected one
(§6) — what is gone is the per-invocation choice of pipeline. The
downstream authority is yog's `docs/REMOTE.md` §5 (*"the engine's driver
keeps no local executor"*) under its §12 front-door invariant; the
priced decision this amendment had to make, and did not assume, is §3.4.

**Section-reference convention.** A bare `§N` in this document names a
section of *this* document; every cross-document reference names its
document — `ARCH §N` for `docs/ARCHITECTURE.md`, anything else by path.

## 1. The ask, and what it is not

yog's client/server split (its `docs/REMOTE.md` §5) needs two things:

1. to place extra tool definitions in front of the model at
   prompt-assembly time — a client-management tool, plus the tools a
   connected remote client advertises;
2. **to be the executor** — to have every invocation the agent makes
   arrive at its own routing leg, so a conversation's tool calls all take
   one road to whichever machine holds the subject.

The second was originally "to route *designated* invocations instead of
the local spawn path", and the widening is bl-a00a's whole substance. It
is not a new capability: a host that wants the old behaviour has it, by
answering a name it does not own the way an absent binary does (§3.3).
What it can no longer do is hand a name back for litany to spawn — which
is what made two pipelines with two containment stories reachable from
one drive.

It is **not** an ask for dynamic discovery inside litany, a second
control plane, or a policy exemption. Everything in §4 stays true.

## 2. Terms

- **Injected tool** — a tool definition a request declares that no
  config named: it is not in the calling role's `providers.yaml` `tools:`
  grant and has no `descriptions/tools/<name>.json` behind it. litany
  already had exactly one source of these, the compactor's
  `write_summary` / `mark_for_deletion` pair (ARCH §2.7). The term is
  litany's own — `src/prompt/dispatch/tools.rs` has called this axis
  *Injection* since bl-f021.
- **Procedure injection** — injected tools contributed by the calling
  role's own procedure. The compactor's pair, and only that, today.
- **Host injection** — injected tools contributed by the **binding**:
  the linked host that called into the command surface. New here.
- **Host** / **binding** — ARCH §3.4's word: the exec binding is
  `src/bin/litany`, a linked binding is a program that links the crate
  and drives the same verbs. Only a linked binding can carry an
  injection; the exec binding passes `None`.
- **Router** — the execution half of a host injection: the function that
  answers every invocation while the injection is installed. It is
  **total** — there is no decline (bl-a00a).
- **Routed invocation** — any invocation under an installed injection.
  Its counterpart is a **spawned invocation**, resolved to a binary by
  ARCH §3.3's three hops, which is every invocation under a binding that
  installed none. Which one an invocation is, is a fact about the
  **binding**, never about the name.

## 3. The seam

One field, one trait, two plain data types. The whole public delta:

```rust
pub struct Fx<'a> {
    // … driver_target, adapter_target, editor, tool_stdin/out/err, stop …
    pub tool_injection: Option<&'a dyn ToolInjection>,
}

pub trait ToolInjection {
    fn tools(&self, workspace: &Path, agent: &str) -> Vec<InjectedTool>;
    fn route(&self, call: RoutedCall<'_>) -> RoutedCapture;
}

pub struct InjectedTool {
    pub name: String,
    pub input_schema: serde_json::Value,
    pub description: Option<String>,
}

pub struct RoutedCall<'a> {
    pub id: &'a str,
    pub name: &'a str,
    pub input: &'a serde_json::Value,
    pub workspace: &'a Path,
    pub agent: &'a str,
    pub cwd: &'a Path,
    pub stop: &'a AtomicBool,
}

pub struct RoutedCapture {
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
    pub exit_code: i32,
}
```

**One object, both halves.** Declaration and permission are not separate
switches. A tool declared and not permitted is announced to the model and
then refused the instant it is called; a tool permitted and not declared
is never called at all. Both failures are unrepresentable here because
`tools()` is the single statement read by prompt assembly *and* by the
grant gate, and `route()` sits on the same object.

**`RoutedCall` is the stdio contract, in memory.** It carries exactly
what a tool subprocess gets — `tool_use.id`, name and input on stdin
(ARCH §3.3 *Stdin*), the `LITANY_CONV_REPO` / `LITANY_CONV_BRANCH`
identity from its environment, and `cwd`, the caller's resolved working
directory a subprocess is *started in* (ARCH §3.3 *Working directory*:
the `cd` mark if it names a live directory, else the worktree — the one
resolution `prepare` performs for both backends, amendment bl-ddaa) —
and one thing a subprocess gets from the kernel instead: the §2.9
cancel flag. `cwd` was added for the host that routes a
worktree-subject tool to a *remote* executor (yog `docs/REMOTE.md` §5:
"an invocation carries its subject's location"): without it a router
must re-derive the mark, a second home for this crate's own fact,
reading a ref namespace §3.3 keeps consumers out of. §3.4's pricing is
untouched — what stays off the seam is the caller's *environment*,
which only an in-process spawning backend could want. `RoutedCapture` is the same three
facts a subprocess produces. Nothing new is invented on either side,
which is what makes §3.1 true.

### 3.1 A routed tool is indistinguishable downstream

The router answers in the stdio contract's own vocabulary, so everything
after the answer is the code that was already there: the result envelope
states `Exit code: N` and marks a non-empty stderr block, `is_error` is
`exit_code != 0`, the `tool_output:` bounded projection caps both streams
before the envelope is rendered, and the executor lands `input.json` /
`output.json` under `steps/<agent-id>/<NNN>/tools/<tool-id>/`. The model
cannot tell a routed tool from a local one, and neither can the
transcript. There is no routed-tool failure taxonomy, because there is no
failure a subprocess could not also have.

### 3.2 The record is the executor's, not the router's

The ball asked that the router "own the per-invocation input/output disk
record like any executor". **It does not, deliberately** — the executor
lands the record around every answer, routed or spawned. Making it the
host's obligation would export ARCH §3.3's record convention (paths,
filenames, atomic-rename discipline, the `ToolOutputRecord` shape) into
the public surface and make an audit trail depend on a host remembering
to write it. Held here it is structural, not disciplinary (PRINCIPLES
"Structure over discipline"), and the surface stays four types wide.

**bl-a00a re-states this, because deleting a spawn could have taken it
with it.** Four things the *executor* owns, and which the inversion moved
by exactly zero inches — they are one implementation
(`spawn/batch.rs::land`) reached by both backends, not two that agree:

| Fact | Owner | Where |
| --- | --- | --- |
| Result envelope (`Exit code: N`, `--- stderr ---`) | executor | `tool/envelope.rs`, via `land` |
| `is_error` == `exit_code != 0` | executor | `land` |
| Bounded projection (`tool_output:`), applied *before* the envelope | executor | `tool/bound.rs`, via `land` |
| `input.json` / `output.json` under `tools/<tool-id>/` | executor | `prepare` and `land` |

`prepare` lands `input.json` and resolves the caller *before* either
backend is entered, and `land` renders and records after either one
returns. A host supplies three facts and never a path, a filename or a
rendering. That is why §3.1 survives the inversion unchanged rather than
being re-argued for it.

The router *is* consulted before the record's `output.json` exists, so a
router that hangs forever leaves an `input.json` and no output — the same
evidence a hung subprocess leaves, and the step's own `response.json`
already names every `tool_use` block regardless.

### 3.3 Router obligations, stated because they cannot be enforced

`route` runs on the executor's own thread. Nothing in the harness can
interrupt it — there is no in-process analogue of the SIGTERM cascade —
so three obligations are stated at the trait and are the host's:

- **Carry your own deadline.** litany imposes no wall-clock limit on a
  tool (ARCH §3.3), and the §6 budgets bound a drive, not a wait. Bound
  every wait and render an expiry as a non-zero `RoutedCapture`.
- **A vanished endpoint is an in-band error result, never a hang.**
  Unreachable, disconnected, protocol garbage: `exit_code != 0` with the
  reason on `stderr`, exactly what an external tool that cannot reach its
  backend does.
- **Watch `RoutedCall::stop`**, so a `litany stop` landing inside a
  routed invocation ends it as promptly as SIGTERM ends a subprocess.

A fourth arrives with bl-a00a's total router, and it is the same
obligation seen from the other side:

- **A name you do not own is a refusal you render**, not a hand-back.
  Non-zero `exit_code`, the reason on `stderr` — indistinguishable from
  the "no such tool" an absent binary produces behind the front door
  (`builtin::Error::Unknown`). A host with nothing to route to therefore
  refuses everything in band, which is the posture working rather than an
  error state (yog `docs/REMOTE.md` §12 *ship inert*).

### 3.4 The priced decision: the exec binding keeps its local spawn

bl-a00a required this resolved explicitly rather than drifted into.
**Ruling: yes — a binding that installs no injection still resolves and
spawns, exactly as before, and that is not a fallback.**

The distinction the downstream invariant actually draws is between *one
pipeline* and *two reachable from one drive*. What made two pipelines a
defect was that a single running agent could hit either, decided by
whether a name happened to be designated — two adjudication stories, two
capture shapes, two containment claims, and no way for an operator to
say which one an invocation took. That is now unrepresentable: the
backend is chosen **once, at the binding, for the whole process**, the
choice is total over names, and `route`'s return type has no shape in
which a call could fall through. Under yog, which installs an injection,
litany spawns nothing — which is the invariant, discharged.

Deleting the spawn outright was weighed and refused, on three counts:

- **It would delete the engine, not a pipeline.** `litany drive` under
  the exec binding would then be structurally unable to run any tool:
  no wire, no registry, no leaf, nothing to install an injection with.
  The engine is a published crate with its own CLI and its own tool
  corpus, and yog's four-component ruling makes it a *component*, not the
  product — a component cannot inherit its composer's deployment
  invariant.
- **Nothing would inherit the corpus.** The subprocess stdio contract,
  the three-hop resolution and the in-process `litany tool <name>` front
  door are what a thrall would have to re-implement to be a tool host at
  all. Deleting them from the engine relocates work rather than removing
  it.
- **Severability cuts the other way** (PRINCIPLES). Policy belongs in the
  capability: *which* pipeline exists is the binding's statement, made by
  installing an object or not. Hard-coding "no local execution" into the
  engine would be the composer's policy compiled into the component,
  removable only by editing code. It is also PRINCIPLES *Integrations are
  external binaries* still standing: a tool is a separate executable with
  a narrow stdio contract, and a host router is that same contract
  answered in memory (§3) rather than a repudiation of it.

The alternative shape — make the local spawn itself an implementation of
`ToolInjection`, so there is literally one backend and the exec binding
installs the local one — is elegant and was rejected for a concrete
reason: a spawning backend needs the caller's **cwd and environment**,
which `RoutedCall` at that time carried neither of. Making it fit would
widen the public seam with fields no host wants, to express an internal
implementation. The seam stays four types wide and the two backends stay
internal, selected by one `Option`. (bl-ddaa later added `cwd` — but as
a *consumer's* requirement, the subject location a routing host puts on
a remote invocation, not as the spawning backend's need: the
environment half stays off the seam, and this ruling stands.)

**What holds the ruling honest**: `SpawnTool` reads `self.injection` in
exactly two places (`execute`, `execute_all`), each a `match` with two
arms and no third, and `src/prompt/tool/tests/injection_scope.rs` pins
the sharp case — a tool binary *installed and resolvable* in the harness
root, called by name, answered by the host anyway.

## 4. Containment

Each of these is a property the seam had to preserve, with the mechanism
that preserves it.

**Individually named tools, never a multiplexer.** `docs/
DESIGN_MCP_BRIDGE.md` §6 ruled that a generic `mcp_call {server, tool,
arguments}` would collapse every downstream boundary — the role grant
(ARCH §4.3), the grant gate, the fork-time descriptor trim, any future
ARCH §3.6 capability policy — into one bit. The ruling stands and now
binds the host too: `tools()` returns individually named definitions,
each with its own schema, and each is granted, adjudicated, declined and
audited by that name. The seam adds no surface a policy would have to
chase.

**The set changes only by the binding's explicit act.** litany never
queries anything to discover tools; it reads what the installed object
states. A host that changes what `tools()` returns has changed the prompt
prefix and pays the cache rebuild knowingly (ARCH §5.5: "between
compactions the assembled prompt only grows at the tail, so provider
prompt caches stay warm"). Nothing in litany makes the set churn — no
poll, no notification receiver, no live catalog — which is the same
property `DESIGN_MCP_BRIDGE.md` §2 got from pinning, arrived at from the
other side: there, discovery is frozen at operator time; here, it is
whatever the embedder holds still.

**Adjudication is untouched.** The tool control (ARCH §3.3 *Tool
control*) is a predicate — pass / refuse / hold — and this ball does not
touch `tool_control` or `tool_step/seam.rs` at all. The order in the
window is unchanged: the grant gate first, then the control, then
execution. Routing happens *inside* execution, so a routed invocation is
adjudicated exactly as a local one, before anything is routed, and a hold
parks it before the router is ever consulted.

**No new verb, no new config key, no new module on the surface.** The
whole public delta is §3's field plus three re-exported types
(`tests/command_surface_parity/` enumerates them). Nothing config-shaped
appears: an injection is a program object, not a declaration, and cannot
be turned on by editing a file.

**A host cannot escalate a role.** Injection widens what may be called by
exactly the names it declares, on every role. That is a real widening and
it is the point — a host-injected tool belongs to the host, not to a
role — but it is bounded by the same in-band decline everything else is:
a name neither granted nor injected is refused at the gate and never
reaches the executor.

## 5. Where it lives

- `src/prompt/tool/inject.rs` — the trait and its three data types. The
  whole seam; re-exported from `src/cmd/mod.rs` because the halves that
  consume it are below the surface and may not name `crate::cmd`.
- `src/prompt/tool/builtin/mod.rs` — `NAMES`, the closed set of names
  the engine performs behind its own front door, re-exported from
  `src/cmd/mod.rs` as `BUILTIN_TOOLS` for the same reason (bl-4cbb). The
  companion fact to the seam: the router's scope is total, so a host must
  be able to ask which names this engine can answer.
- `src/prompt/tool/spawn.rs` + `spawn/batch.rs` — the two backends.
  `execute` and `execute_all` each `match self.injection` exactly once,
  choosing `route`/`route_fan` or `spawn_one`/`spawn_fan` for the whole
  answer; `prepare` runs ahead of both and `land` is the one landing both
  share.
- `src/prompt/dispatch/tools.rs` — `injected(role, executor)` is where
  the procedure and host sources meet, and `compose` splices that list
  ahead of election.
- `src/prompt/dispatch/tool_step/permit.rs` — the grant gate unions the
  same list's names.
- `src/cmd/prompt.rs`, `src/prompt/dispatch/advance/cli.rs` — the two
  production wirings, both `SpawnTool::new(…).with_injection(…)`.

**Why the executor carries it and `Deps` does not.** The composer and the
grant gate need the *definitions*; the execute path needs the *router*.
Putting the object on `Deps` beside the executor would give one fact two
homes and a way to disagree. Putting it on the executor gives the honest
reading: the thing that will answer a call is asked what it can answer,
and prompt assembly derives the declaration from that (PRINCIPLES, single
source of truth; "derive don't mirror").

**Why the compactor's injection was generalized.** It was two functions
— a schema half and a names half — held in step by a test.
`compactor::builtin_tool_schemas(role)` now returns the same
`Vec<InjectedTool>` a host does, `compactor::injected` is deleted, and
both consumers read one list two ways. The generalization subtracts code
and makes procedure and host injection literally one mechanism with two
sources, rather than two mechanisms that rhyme (PRINCIPLES, "One obvious
path").

## 6. Precedence: an injected name outranks an elected one

A host may declare a name the calling role also grants. Two entries for
one name is a request some providers refuse outright, so the composer
must choose, and it chooses injection — in *both* halves. The declaration
carries the injected schema, and under bl-a00a the router answers
whatever is called, so the schema the model reads is always the schema of
the thing that will run. The ordinary case is disjoint sets, where the
rule is invisible.

**bl-a00a makes the execution half of this trivially true rather than
merely arranged.** It used to hold because the router was consulted
*before* resolution and could take a name over; now there is no
resolution to outrank. What survives is the declaration half, which is
where the rule was always doing work: it is the composer that must not
emit two entries for one name.

The inverse — a host that declares a name and then does not implement it
— is not policed and is now unambiguous: it is a refusal the host itself
renders (§3.3's fourth obligation), non-zero and in band, which is what
an absent binary produced behind the front door too. It is no longer a
`None` that means something else somewhere.

## 7. What this does not solve

- **Nothing bounds a router in wall-clock time.** §3.3's obligations are
  stated, not enforced, and a host that ignores them wedges the drive
  that installed it. The alternative — running the router on a watchdog
  thread — would put a `Sync` bound on the injection and, through it, on
  whatever the host holds behind it; that price buys protection against
  the host's own bug in the host's own process. Declined, and stated.
- **Parallel fan-out under an installed host is now wholly serial, and
  bl-a00a widened that** (previously only the routed subset was serial
  and the spawned remainder still overlapped). Under a `parallel`
  multi-tool envelope the router answers every call in list order on the
  calling thread, so a `parallel` envelope of N calls costs N round trips
  end to end. It stays this way because `route` runs on the executor's
  own thread by construction: overlapping it would put a `Sync` bound on
  the injection, and whether a host's transport is safe to drive
  concurrently is the host's fact, which litany holds none of. If it is
  ever paid for (filed downstream as yog bl-fab6), the shape is a
  **defaulted** `route_all(&self, calls)`
  on the trait, mapping over `route` unless a host overrides it — purely
  additive, so no host is broken by its arrival, and no host is asked for
  a concurrency guarantee it cannot give. Nothing observable changes
  today: results still render in list order.
- **The seam is per-process, not per-agent — but every question it is
  asked names the agent** (amended bl-ddaa). One injection object serves
  every agent a driver verb drives. `RoutedCall` carries the workspace
  and agent id so a router can discriminate, and since bl-ddaa `tools()`
  is handed the same pair — a request is always assembled *for* one
  agent, and a host whose declared set is per-agent state (a
  loaded-tools document keyed by agent) otherwise had to read the agent
  off its own argv, which a verb that *mints* its agent cannot do: the
  minting driver then declared nothing for its whole drive while its
  loads kept promising callability. What litany still will not do is
  hold or merge per-agent state for the host — the discriminants cross,
  the discrimination is the host's.
- **Procedure-injected tools are routed like any other, and the host
  answers the compactor pair itself.** The pair (`write_summary` /
  `mark_for_deletion`, ARCH §2.7) reaches the executor as an ordinary
  `tool_use` and is therefore answered by an installed router, exactly as
  every other name now is — no exemption, no second road. This was filed
  open (bl-a00a listed three candidates and declined to pick, because the
  invariant at stake — yog `REMOTE.md` §5, *"every tool call the agent
  makes takes the same road"* — is not litany's to amend). **It is
  adjudicated: the composer ruled the first candidate, and the ruling's
  home is yog's `docs/REMOTE.md` §5.4** (landed against the 0.0.2 pin).
  The reasoning recorded there, because it is the part a later reader
  needs:
  - **Subject locality decides it alone** (`REMOTE.md` §5, *"a tool
    executes where its subject lives"*). The pair's subject is the agent
    itself: `write_summary` writes that agent's own summary onto the
    compactor branch and `mark_for_deletion` nominates that same agent's
    own files. The agent lives on the server, so no machine and no thrall
    is in the picture.
  - **yog's §12 *front door only* is therefore not narrowed.** It governs
    execution *on a machine*, which the pair is not — so the carve-out
    this bullet once worried about is not a carve-out in the invariant at
    all. The principle it falls out of: context management happens in the
    composing host.
  - **Almost nothing is asked of litany.** The surface a host needs
    already existed and is the one yog used — the `tool` verb on the
    public `cmd::Command` surface, the same front door ARCH §3.3's third
    resolver hop addresses as `<driver_target> tool <name>`. One thing
    was missing and is now there (bl-4cbb): *which names* that door
    answers to. A host was restating the set from memory, and no gate on
    either side of the crate boundary could see that restatement go
    stale — an eighth built-in would simply refuse on a host that never
    heard of it, in a voice that reads as the host's. `cmd::BUILTIN_TOOLS`
    is the same const the decline and the `--help` render, so there is
    one list and the host reads it. A host
    re-enters it with the caller identity on the child's environment
    (`LITANY_CONV_REPO` / `LITANY_CONV_BRANCH`, ARCH §3.3, taken from
    `RoutedCall`'s own `workspace` / `agent`) and the `tool_use` input on
    stdin, so the compactor's semantics keep exactly one definition and
    it is this repo's.

  **The residual, which is a property of the front door and not a
  defect:** the built-ins resolve the calling agent's worktree from the
  **process** environment (`builtin::run` wires `dispatch::ProcessEnv`),
  so an in-process `Command::Tool` cannot carry a per-invocation
  identity — a host that links litany must still re-enter as a child
  process to answer the pair, and cannot answer it by linking alone.
  That is what keeps the identity harness-derived rather than
  model-supplied (ARCH §2.11), and it is the one thing a composing host
  has to rediscover if this paragraph is not read.
- **No stability promise.** Like the linked binding and the mint seam
  (`src/mint.rs`), this is pin-exact 0.x consumption: no semver
  stability.
