# Design: host tool injection at the binding (bl-9001)

**Ruling: one optional object on `cmd::Fx`, carrying both halves — extra
tool definitions and an execution router — and nothing else moves.** A
linked binding may put tools of its own in front of the model and answer
them itself; the pool, the grant model, the tool control, the result
envelope, the disk record and the prompt-cache discipline are all
untouched. This document records the seam, defines its terms, and states
what it is contained by — in particular why it does not reopen the
`docs/DESIGN_MCP_BRIDGE.md` refusals.

**Section-reference convention.** A bare `§N` in this document names a
section of *this* document; every cross-document reference names its
document — `ARCH §N` for `docs/ARCHITECTURE.md`, anything else by path.

## 1. The ask, and what it is not

yog's client/server split (its `docs/REMOTE.md` §5) needs two things:

1. to place extra tool definitions in front of the model at
   prompt-assembly time — a client-management tool, plus the tools a
   connected remote client advertises;
2. to route designated invocations to its own engine instead of the
   local spawn path, so a conversation can teleoperate another machine's
   advertised tools.

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
- **Router** — the execution half of a host injection: the function
  consulted for every invocation, which either answers it or declines to
  own it.
- **Routed invocation** — one the router answered. Its counterpart is a
  **spawned invocation**, resolved to a binary by ARCH §3.3's three hops.

## 3. The seam

One field, one trait, two plain data types. The whole public delta:

```rust
pub struct Fx<'a> {
    // … driver_target, adapter_target, editor, tool_stdin/out/err, stop …
    pub tool_injection: Option<&'a dyn ToolInjection>,
}

pub trait ToolInjection {
    fn tools(&self) -> Vec<InjectedTool>;
    fn route(&self, call: RoutedCall<'_>) -> Option<RoutedCapture>;
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
(ARCH §3.3 *Stdin*), plus the `LITANY_CONV_REPO` / `LITANY_CONV_BRANCH`
identity from its environment — and one thing a subprocess gets from the
kernel instead: the §2.9 cancel flag. `RoutedCapture` is the same three
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
- `src/prompt/tool/spawn.rs` + `spawn/batch.rs` — the router is consulted
  ahead of the §3.3 resolution order, per invocation, in `execute` and in
  `execute_all`. `land` is the one landing both backends share.
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
carries the injected schema, and the router is consulted before
resolution, so the schema the model reads is always the schema of the
thing that will run. The ordinary case is disjoint sets, where the rule
is invisible.

The inverse — a host that declares a name and then declines to route it —
is not policed: it lands as an ordinary "no such tool" decline behind the
front door (the §3.3 third hop), non-zero and in band, which is what an
absent binary does too.

## 7. What this does not solve

- **Nothing bounds a router in wall-clock time.** §3.3's obligations are
  stated, not enforced, and a host that ignores them wedges the drive
  that installed it. The alternative — running the router on a watchdog
  thread — would put a `Sync` bound on the injection and, through it, on
  whatever the host holds behind it; that price buys protection against
  the host's own bug in the host's own process. Declined, and stated.
- **Parallel fan-out over routed invocations is serial.** Under a
  `parallel` multi-tool envelope the router answers what it owns in list
  order on the calling thread, then the spawned remainder overlaps as
  before. Whether a host's transport is safe to drive concurrently is the
  host's fact and litany holds none of it. Results still render in list
  order, so nothing observable changes.
- **The seam is per-process, not per-agent.** One injection serves every
  agent a driver verb drives. `RoutedCall` carries the workspace and
  agent id so a router can discriminate, but litany will not do it for
  them.
- **No stability promise.** Like the linked binding and the mint seam
  (`src/mint.rs`), this is pin-exact 0.x consumption: no semver
  stability.
