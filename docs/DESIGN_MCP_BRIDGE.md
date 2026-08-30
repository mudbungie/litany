# Design: MCP client bridge as an external tool (bl-3c76)

**Ruling: deployment-owned bridge, no litany product code.** The existing
`litany-tool-<name>` external-tool seam (ARCHITECTURE §3.3) carries a Model
Context Protocol (MCP) client bridge with **zero changes to litany** — no new
verb, no new config key, no new channel. MCP integration is a deployment
concern in exactly the way provider wire protocols are brazen's
(ARCHITECTURE §4.4, PRINCIPLES "Integrations are external binaries"): the
harness owns
orchestration and on-disk state; the bridge owns the MCP wire protocol, server
quirks, and credentials. This document settles the seven questions the ball
posed, records the refusals, and defines the bridge's contract so any
deployment can build one that composes with the harness unchanged.

MCP itself — host / client / server, the three server primitives, the tools
capability — is already defined in `docs/TAXONOMY.md` §4 *Model Context
Protocol (MCP)*; this document coins only **pin** (§2 below) and re-specifies
nothing the taxonomy owns.

**Section-reference convention.** A bare `§N` in this document always names a
section of *this* document; every cross-document reference names its document
— `ARCHITECTURE §N` for `docs/ARCHITECTURE.md`, and anything else by path
(`docs/TAXONOMY.md` §4). This matters most at §6: this document has one and so
does ARCHITECTURE (*Workflow as Configuration*, whose budgets are cited
below), and they are unrelated.

## 0. The seam, verified against current source

Every premise below was checked against the tree at `main` (cad4b2a):

- **External binary slot.** The harness resolves an external tool as
  `<data-root>/tools/litany-tool-<name>`, falling back to `litany-tool-<name>`
  on `PATH` (`src/prompt/tool/spawn.rs`; `EXTERNAL_PREFIX` in
  `src/prompt/tool/mod.rs`). The stdio contract (ARCHITECTURE §3.3) is:
  `tool_use.input` JSON on stdin, product on stdout, diagnostics on stderr,
  exit code mapped to
  `is_error`, `LITANY_CONV_REPO` / `LITANY_CONV_BRANCH` in the environment,
  cwd = the calling agent's working directory, SIGTERM with a 5-second flush
  deadline on cancel.
- **Tool triple.** A tool is binary + JSON schema (`<data-root>/tools/<name>.json`)
  + skill (`<data-root>/skills/<name>/SKILL.md`). Any on-disk triple is
  discoverable; role configs select from the pool (ARCHITECTURE §3.3
  *Tools-list assembly*).
- **Descriptor snapshot.** Config-commit authoring (`litany new` /
  `litany config`) snapshots the data-root pools into the config commit's
  `descriptions/**`; branches inherit the snapshot via git and assembly
  intersects against the branch's own read-state tree, never live data-root
  state (ARCHITECTURE §3.3 *Descriptions-always population* — snapshot, not
  mirror). The fork trims `descriptions/tools/` to the role's grant (bl-a900).
- **Result envelope.** `Exit code: <N>` + stdout + labelled stderr, bounded by
  the `tool_output:` projection (bl-ffc5, bl-d5fa).
- **No wall-clock limit.** The executor imposes none; only the ARCHITECTURE
  §2.9 cancel cascade and ARCHITECTURE §6 budgets bound a tool invocation's
  duration.
- **bl-ae6b's builtin-vs-external reasoning** (ARCHITECTURE §3.3 *Why
  in-process, not a `litany-tool-*` binary*) and the ARCHITECTURE §3.6
  trusted-computing-base line are quoted and applied in §1 below.
- **The named deployment.** The fleet demo (`~/ops/fleet`) already ships
  external Slack tools as full triples — `tools/litany-tool-slack_read`,
  `tools/litany-tool-slack_post`, schemas and skills beside them — over a
  *mock* channel (a flock-guarded NDJSON file). The seam shape is proven; only
  the backend is fake. §7 makes that the acceptance story.

## 1. Placement: outside the repo, beside the deployment

Three candidate homes, one survivor:

**In litany core (in-process built-in): refused.** ARCHITECTURE §3.6 states
the criterion: "Shipping a tool in-process is the decision to place it in the
trusted
computing base." bl-ae6b's defense of the in-process `apply_patch` rested on
that criterion cutting the *other* way: "`apply_patch`'s authority (write
files where the invocation's cwd points) is strictly a subset of what the
in-process `bash` already wields, so externalizing it would bound nothing."
An MCP bridge is the inverse case on every axis: it holds live connections to
third-party servers, third-party credentials, and code paths (JSON-RPC
framing, server process management) driven by remote input. Nothing about
that belongs in the trusted computing base, so the same test that pulled
`apply_patch` in pushes the bridge out.

**In-repo external binary (first-party `litany-tool-mcp` shipped by litany):
refused.** bl-ae6b's other argument — "the external slot is the *contributor*
channel — shipping a first-party tool through it would open a second
distribution path for core code" — reads at first as pressure to pull any
first-party tool in-process. It is actually the argument against this option:
a first-party bridge would make the litany repo ship, version, and gate code
whose entire subject matter is other people's wire quirks. The precedent is
already in the same paragraph: "the provider layer's per-name binaries were
retired ... once brazen gave the layer a real interface." Integration breadth
lives *outside* the repo behind a narrow contract — the harness never grew a
first-party provider tool, and it should not grow a first-party MCP tool. The
ball's own framing agrees: breadth alone is not a coding-quality gap, and
core inclusion was refused up front.

**Deployment-owned bridge: the ruling.** The bridge is a binary the
*deployment* installs into the `litany-tool-<name>` slot, exactly as the
fleet demo installs its Slack tools today. litany's severability test passes
perfectly: deleting MCP support deletes deployment files (the bridge binary,
its config, the pinned triples), not a line of litany code — because no line
of litany code exists. The division of labor is brazen's, verbatim from
PRINCIPLES: "the binaries own wire protocols, vendor quirks, and credential
handling — litany never sees a credential."

A first-party *sibling* product (a brazen-style `~/dev/` repo with its own
spec) is the plausible future if two or more deployments want the same
bridge; that is a promotion decided by demand, not now. Nothing in this
design changes when it happens — the seam is the contract, and the contract
is already shipped.

## 2. Descriptor pinning: dynamic discovery becomes the ordinary tool pool

**Coined term — pin.** To **pin** an MCP tool is to translate it, once, at
operator time, into an ordinary litany tool triple in the data-root pools. A
**pinned tool** is the result: indistinguishable from any hand-authored
external tool. (Definition lives here; used nowhere in litany code.)

The bridge ships an operator verb (say `bridge pin`, run by a human or a
deployment's install script — the same trust posture as `bz --login`,
operator-run, never harness-run) that:

1. starts each configured server, performs the MCP `initialize` handshake,
   and requests `tools/list`;
2. for each tool on the bridge config's **allowlist** (never the whole
   catalog — §6 below), writes the triple:
   - **binary**: a symlink (or trivial exec shim) `litany-tool-<name>` →
     the bridge binary, which reads its own invoked name to select the
     pinned server+tool;
   - **schema**: the MCP tool's `inputSchema`, written to
     `<data-root>/tools/<name>.json` verbatim (it is already JSON Schema);
   - **skill**: `<data-root>/skills/<name>/SKILL.md`, frontmatter
     `description` from the MCP tool description, body carrying any usage
     notes the operator authors;
3. declines, loudly, any allowlisted name that is absent from the server's
   catalog, collides with an existing pool entry, or is not a valid single
   path component — never fuzzy-matched, never renamed silently (PRINCIPLES
   "Decline illegal operations"). Where the deployment wants prefixing
   (`slack_` etc.) it is stated in the bridge config, not invented.

**No duplicate registry, by construction.** The data-root pool *is* the
registry litany already has; the pin step populates it and nothing else. The
committed `descriptions/**` snapshot — taken by the existing config-authoring
step, unchanged — is what freezes the pool a branch sees: assembly reads the
branch's own tree, so that pool is a pure function of its fork point. Dynamic
discovery happens exactly once, at pin time, as an operator act.

**Cache stability falls out rather than being arranged.** Prompt assembly is
append-only and cache-priced (ARCHITECTURE §5.5); a tool list that churned per
run would
flush the prompt cache on every drift of a remote server's catalog. Pinning
makes churn structurally impossible: the MCP server's live catalog is not an
input to assembly at any point. `notifications/tools/list_changed` has no
receiver — there is no resident client to receive them (§3 below) — and that
is a
feature, not a gap. Picking up a server's new or changed tools is re-running
`pin` and authoring a new config commit; branches forked from older configs
stay pinned to what they saw, the same "fork is the freeze" discipline as
every other descriptor (ARCHITECTURE §3.3). Drift between a pinned schema and
a server's
live schema surfaces as an ordinary in-band tool decline at invocation time
(the server rejects the arguments; the bridge exits non-zero with the
server's error on stderr), which is the honest failure: re-pin and re-author.

## 3. Server lifetime: one server per tool invocation

litany runs one tool subprocess per tool invocation; an MCP server wants to
stay resident. The ruling: **the server's lifetime is contained in the
invocation's.** Per invocation, the bridge spawns the configured server as a
child process, performs `initialize`, issues one `tools/call`, renders the
result, and tears the server down (close stdin, bounded wait, kill). MCP's
per-connection state begins and ends inside the invocation.

Why this and not a resident server:

- **Regenerability.** "Any process can die at any time without losing state.
  ... No process is load-bearing; disk is" (PRINCIPLES). A resident MCP
  server would be exactly the load-bearing daemon the architecture spent
  itself eliminating — crash recovery, liveness probing, and stale-connection
  reconnect logic for a component litany cannot even see.
- **Precedent.** brazen is one `bz` process per attempt; nobody keeps an
  HTTP/2 connection warm across model calls, and the shape has held. The
  bridge is the same shape one layer over.
- **Failure semantics collapse.** Startup failure, handshake failure, and
  mid-invocation death are all one observable: a non-zero exit with stderr,
  carried to the model by the result envelope. There is no "reconnect"
  because there is no connection to lose between invocations.

**Semantics, pinned:**

- **Startup.** Server spawn + `initialize` per invocation. The executor
  imposes no wall-clock limit (ARCHITECTURE §3.3), so a slow server start (an
  `npx`-hosted server, cold caches) is latency, not breakage; ARCHITECTURE §6
  budgets bound the pathological case.
- **Cancellation.** Harness SIGTERM → bridge; the bridge terminates its
  server child (own process group) and exits within the 5-second flush
  deadline. No MCP `notifications/cancelled` choreography — the connection
  dies with the process, which is the whole cancellation story.
- **Failure mapping.** Transport or protocol failure → bridge exits non-zero,
  diagnostic on stderr. A successful `tools/call` whose result carries
  `isError: true` → result content on stdout, non-zero exit (the MCP
  tool-level error *is* the tool failing; `is_error` should say so). A clean
  result → content on stdout, exit 0. Structured content is emitted as the
  JSON it is; the envelope already refuses double-encoding.
- **Cross-invocation state.** None in the bridge. A server whose usefulness
  depends on remembering earlier invocations must keep that state on its own
  disk (most real servers front stateless HTTP APIs and need nothing). A
  server that *cannot* work per-invocation is out of scope for the stdio
  bridge, named as such in the bridge's docs, and is the concrete evidence
  that would justify the deferred option below.

**Deferred, explicitly: a warm-server option.** If a named deployment
*measures* per-invocation startup as a defect (numbers, not vibes), the
bridge may internally keep a warm server behind a local socket. That is
bridge-owned deployment infrastructure — invisible at the seam, exactly as a
database a tool queries is — and changes nothing in this design or in
litany. It is not built now, because no deployment has charged the cost.

## 4. Transport: stdio, and only stdio

The bridge speaks MCP's stdio transport to servers it spawns itself. HTTP
transports, remote servers, and OAuth flows are refused until a named
deployment requires a specific remote server — at which point they are a
bridge-config change (the bridge owns transport, as brazen owns auth modes),
still not a litany change. The seam does not move; that is the point of the
seam. One transport until one deployment proves it, per the ball's refusal.

## 5. Credentials: the bridge's, never litany's

- **Ownership.** Server commands, arguments, and credential material
  (tokens in env vars, or a credstore if the bridge grows one) live in the
  bridge's own deployment-owned config, mode 0600 — the brazen division,
  verbatim: interactive acquisition is operator-run; the harness never
  prompts and never sees credential material. Nothing credential-shaped
  rides litany config, litany env, or the tool's input schema.
- **Redaction.** Everything a tool prints on stdout or stderr enters the
  committed transcript via the result envelope (bounded, but committed). So
  the bridge's discipline is: never echo server config or environment into
  diagnostics; scrub its known secret values from any error text it emits
  (spawn failures love to quote the command line — the bridge must not).
- **Residual risk, named.** A pinned tool's *result* is the server's own
  product; a server that echoes a credential into its result has put it in
  the transcript, and no bridge mechanism can prevent that. The mitigation
  is the same as ARCHITECTURE §3.6's supply-chain paragraph: the pin allowlist
  is the
  operator vouching for each tool, a decision made once and auditable at a
  glance. Provenance checking of servers is out of scope here as it is
  there.

## 6. Capability metadata: pinned tools are ordinary tools, so policy covers them

**The load-bearing ruling: one litany tool name per pinned MCP tool — never a
generic `mcp_call {server, tool, arguments}` multiplexer.** A multiplexer
would collapse every downstream boundary to one bit: the role `tools:` grant
(ARCHITECTURE §4.3), the grant gate (declaring is not permitting, bl-5a1f),
the fork-time descriptor trim (bl-a900), and any future ARCHITECTURE §3.6 /
capability-boundary policy would all see "the MCP tool" where the operator
means "post to Slack but do not delete channels." Per-tool naming keeps every
one of those
mechanisms exact, with no MCP special case anywhere. This is what "no bridge
that bypasses policy" means structurally: the bridge adds no surface that
policy would have to chase.

- **Today (v1.0).** A pinned tool is granted, refused, trimmed, and audited
  exactly as `slack_read` is in the fleet demo. Nothing new.
- **v1.1 sandbox (ARCHITECTURE §3.6).** A pinned tool's binary is a native
  executable, so it loads only under a role granting `exec`, and then runs
  unclamped —
  coarse, honest, and stated. A future bridge compiled to `wasm32-wasip2`
  with `net` imports would clamp per-host; that is an artifact decision the
  bridge's builder makes, not a litany decision.
- **The yog capability-boundary ruling (yog bl-0cea, open as of 2026-08-02,
  blocked on yog bl-2b8c).** That design will settle effect vocabulary and
  enforcement for external tools generally. Because pinned tools present as
  ordinary external tools, whatever it rules covers them with zero MCP
  special-casing — which is precisely why this design refuses any shape
  (multiplexer, resident daemon, side channel) that would make MCP a case
  that ruling would have to distinguish. If bl-0cea demands per-tool effect
  metadata (read vs destructive vs network), the pin step is where an
  operator authors it, since pin is where a human already vouches per tool.

**The ruling now binds a second consumer (bl-9001).** litany has since
grown a **host tool-injection** seam: a linked binding may declare tools
of its own and route their invocations
(`docs/DESIGN_TOOL_INJECTION.md`, ARCHITECTURE §3.3 *Host-injected
tools*). It was built to this section's ruling rather than around it —
the injected tools are individually named, each with its own schema, so
the grant gate, the descriptor trim and any future capability policy
still see one name per capability, and a host multiplexer is refused for
exactly the reason a bridge one is. Nothing in §1 moves: that seam is a
*binding* facility, carries no wire protocol, no credential and no server
lifetime, and an MCP bridge remains a deployment-owned binary in the
`litany-tool-<name>` slot. A host that wanted to speak MCP through the
new seam would be putting the bridge inside its own process, where its
own trusted computing base absorbs the cost — still not litany's.

## 7. Token cost, lazy discovery, and the acceptance story

**Cost model.** Each pinned tool costs its schema + frontmatter description
in `descriptions/**`, composed on every model call for roles that grant it —
and *only* those roles, since the fork trims descriptors to the grant. The
skill body costs nothing until an agent elects `load_skill`. So the token
policy is the existing one: curation at pin time (the allowlist), grant
granularity per role, progressive disclosure per skill. **Never pin a
server's whole catalog** — a typical SaaS MCP server exposes dozens of tools
at a few hundred tokens each, and an unpinned tool costs exactly zero.
"Lazy discovery" in the MCP sense (live catalog queries mid-run) is refused
outright: it is the cache-instability failure §2 exists to prevent, and the
allowlist is the lazy mechanism — you discover at pin time, cheaply, once.

**Acceptance: the fleet Slack swap.** The named deployment already exists.
`~/ops/fleet` ships `litany-tool-slack_read` / `litany-tool-slack_post` as
complete triples over a mock NDJSON channel; its `sensor` role is granted
`slack_read` today. Acceptance for the bridge is the backend swap:

1. Bridge config names a real Slack MCP server (stdio, spawned locally) with
   an allowlist of exactly `slack_read`-and-`slack_post`-equivalent tools.
2. `bridge pin` writes the triples into the deployment's data root; `litany
   config` (or a fresh `litany new`) snapshots them.
3. The fleet's roles and grants are edited only to name the pinned tools; a
   sensor agent invocation round-trips a real channel read, and a worker
   posts, through the ordinary envelope — exit codes, stderr labelling,
   bounded projection all intact.
4. **The pass criterion is a diff:** `git -C ~/dev/litany diff` is empty.
   The whole exercise lands in deployment space.

## 8. Refusals (restated from the ball, now with mechanisms)

- **No MCP in litany core** — §1; the TCB test and the brazen precedent.
- **No marketplace** — pin is an allowlist an operator authors; there is no
  browse, no install verb, no catalog surface.
- **No second transport before a named deployment** — §4.
- **No resident server / daemon** — §3; regenerability.
- **No generic multiplexer tool** — §6; grant granularity is the policy
  boundary.
- **No live catalog queries** — §2, §7; pin-time discovery only.

## 9. Follow-up: filed, then closed unworked

**bl-8925** filed the §7 acceptance run — prove the seam by swapping the fleet
demo's mock Slack tools for MCP-pinned ones, deployment-space only, closing
evidence the empty-diff criterion. It was **closed unworked on 2026-08-06**:
MCP integration is not a current priority, and the two things it needed — a
chosen Slack MCP server and a workspace credential — were never supplied.

Nothing above depends on that run having happened. The ruling (§1), the
lifetime, transport, credential and capability semantics (§3–§6) and the
refusals (§8) are settled on the seam as it already ships; §7 remains the
specification of what a proof would consist of, available to whoever next
needs a real MCP-backed tool. What is *not* claimed is empirical: no bridge
has been built and no MCP server has been round-tripped through the envelope,
so this document is a design checked against the source, not a report of a
working integration.

No litany implementation ball is filed, because the design concludes none is
warranted: the seam shipped with ARCHITECTURE §3.3, and this document is the
record that
it was checked, end to end, against a bridge-shaped consumer.
