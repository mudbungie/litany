# Design: code execution — a program composes granted tools (bl-acee)

**Ruling: one new built-in, `python`, runs a model-authored python3 program
beside the engine; the program reaches every tool the agent is granted through
one new front-door verb, `litany invoke`; each of those inner invocations is
gated, adjudicated and recorded exactly as a top-level one, and none of them
enters the model's context — only the program's final stdout does, through
the ordinary result envelope. The multi-tool retires into it. No other
built-in is added: every remaining gap in the standard corpus is a shell line,
a program, a file in the worktree, or a deliberate refusal.**

Source lesson (the umbrella, bl-8175, lesson 4): a child program imports
generated tool stubs; its tool calls go back to the parent over RPC and run
through the normal dispatcher; only the program's final stdout enters model
context; loops, joins, filtering and dozens of invocations cost one round trip,
and policy stays in the parent. Three operator rulings bind this document:
python is the scripting language; context is scarce, so only a program's final
stdout enters context; look at standard workflows and tools and implement what
you can.

**Section-reference convention.** A bare `§N` names a section of this document;
`ARCH §N` is `docs/ARCHITECTURE.md`, `PRINCIPLES` is `docs/PRINCIPLES.md`,
anything else by path. Terms coined here are entered in `docs/TAXONOMY.md` §4.

## 1. Terms

- **Program** — the python3 source one `python` invocation runs: a string the
  model authored, carried in the tool's input as `program`.
- **Stub module** — `litany_tools`, a python module the built-in generates for
  each program from the effective toolset (ARCH §3.3, *declaring is not
  permitting*): one function per granted tool, its parameters read from the
  tool's committed schema, its docstring the tool's description.
- **Inner invocation** — already coined for the multi-tool (`docs/TAXONOMY.md`
  §4): one `{name, input}` the model minted no wire id for, run through the
  same grant gate, control and executor as a top-level `tool_use`, recorded
  under a derived id. A program's tool calls are inner invocations — the
  multi-tool's list was one written ahead of time, a program's is written as it
  runs. The term is widened, not replaced.
- **Door verb** — `litany invoke`: the front door for one inner invocation
  (§2.2). Distinct from `litany tool <name>`, which is a tool's *binary* (ARCH
  §3.3 resolver hop 3) and gates nothing.

## 2. A — the code-execution tool

### 2.1 The channel is the front door, and the door is a new verb

The gap analysis found the shape already half-present: `litany tool <name>` is
a uniform re-entry point, and an agent's `bash` could shell out to it today.
What that path bypasses is everything the tool window is for — the grant gate
(`src/prompt/dispatch/tool_step/permit.rs`), the tool control seam
(`tool_step/seam.rs`), the diagnostic record, and under a linked binding the
host's router, which since bl-a00a *is* the executor with nothing spawning
behind it. Widening `litany tool` to gate its own invocation would make hop 3
of the resolver adjudicate what hops 1 and 2 do not; that is two contracts on
one name.

So the door is a verb of its own. **`litany invoke`** reads one `tool_use`
block — `{id, name, input}`, the same object a tool control reads on stdin
(ARCH §3.3 *Tool control*) — from stdin, resolves the calling agent from
`LITANY_CONV_REPO` / `LITANY_CONV_BRANCH` exactly as the built-ins do, and runs
that one invocation through the window's gates in order: the depth refusal,
the grant gate, the control consult, then the executor — spawning under the
exec binding, routed under a linked one. It prints the invocation's **raw**
result envelope on stdout (exit code line, stdout, stderr under its marker,
ARCH §3.3 *Result envelope*) and exits with the tool's exit code. It commits
nothing: an inner invocation's worktree side effects ride the enclosing
`python` invocation's one tool commit, as the multi-tool's already do
(ARCH §3.3 *Commit-per-side-effect*).

The body of the verb is the multi-tool's `run_inner`
(`src/prompt/dispatch/tool_step/multi/inner.rs`) lifted to the command
surface — the same gates, in the same order, with the same hold rule: **a hold
cannot park mid-program**, because entries before it have run, so an inner
`hold` degrades to an in-band decline telling the model to re-issue that
invocation top-level, where a hold parks properly. Nothing new is decided
there; the paragraph in ARCH §3.3 *The multi-tool* is the rule and moves with
the code.

The envelope is printed unbounded. The bounded projection (ARCH §3.3, bl-d5fa)
bounds what enters the *transcript*; an inner result enters a program, which
is exactly the consumer that can filter it. Its full bytes are on disk in any
case (§2.3).

*Why a verb and not an in-process sidechannel.* PRINCIPLES *Everyone uses the
front door*: "every procedure-to-procedure invocation … goes through it.
Nothing else — no divergent library API, no in-process sidechannel, no ad-hoc
socket." A pipe protocol between the interpreter and the tool window would be
that socket. A verb also makes the program testable in isolation — a fixture
program against a fixture door — and makes the stub module trivially small:
every generated function is one `subprocess.run` of the verb.

### 2.2 How a program reaches the door

`python` is an in-process built-in (`litany tool python`, `builtin::NAMES`).
It receives `{program}` on stdin, generates the stub module, runs
`python3 -` with the program on stdin and the module on `PYTHONPATH`, and
relays stdout, stderr and the interpreter's exit code — the ordinary stdio
contract, nothing added. The generated module bakes in two facts the built-in
already holds: the **driver target** (`cmd::Fx::driver_target`, the same
injected path resolver hop 3 and the `dispatch` built-in re-enter with — never
a `litany` resolved by name, ARCH §2.11) and the enclosing invocation's
`tool_use.id` (§2.3). So the program's environment carries nothing new for
the door's sake, and no path is written into the worktree.

Under a linked binding the driver target is the host's re-exec target, so the
stub execs `<host> invoke`; the host re-enters the verb with its injection
installed and its router answers the inner invocation as it answers every
other (`docs/DESIGN_TOOL_INJECTION.md` §7 gains the obligation; the yog half
is filed there, §6).

### 2.3 Attribution: one env var joins the stdio contract

Inner records land at `steps/<agent-id>/<NNN>/tools/<tool-id>-<k>/` beside the
program's own record — the multi-tool's derivation, with `k` minted by the
stub module in program order. The built-in must therefore know its own
`tool_use.id`, and today no tool does: the stdio contract hands a tool its
input and the calling agent's identity, never the invocation's. **The
contract gains `LITANY_TOOL_ID`** — the `tool_use.id` of the invocation being
executed, set by the executor from the same `ToolCall` it records, on every
spawn. It is the same class of fact as `LITANY_CONV_BRANCH` (ARCH §3.3
*Environment*: identity a tool needs "without the model having to thread
context through the input schema"), and a routing host owes it on the spawns
it makes, as it already owes `cwd` (`RoutedCall` carries `id`). From the id
and the agent the built-in derives its record directory — the in-flight step
is the highest-numbered `steps/<agent-id>/<NNN>/`, a derivation and not a
stored cursor — and writes the stub module there, so *what the program could
call* sits beside *what it did call*, both diagnostic, neither in context.

### 2.4 What the model sees

One `tool_result`, for the `python` invocation: `Exit code: N`, the program's
stdout, its stderr (a traceback lands there naturally) under the marker, each
stream head-and-tail bounded per `tool_output:`. **No inner invocation's
result enters the transcript** — not a line, not a tally — because the
program's stdout is the model's whole reading of what happened, by the
operator's ruling. The failure projection is therefore the existing one: a
non-zero interpreter exit sets `is_error`, and the bounded stderr tail carries
the traceback's last frames, which is the part a model acts on. A missing
interpreter is the same in-band failure a missing binary is under `bash`
(exit 127, stderr naming it).

### 2.5 Timeout

None per invocation, as for `bash` (ARCH §3.3: "the executor imposes no
wall-clock limit — only the §2.9 cancel cascade"). The two bounds that exist
are the ones that bound `bash`: `litany stop` (ARCH §2.9 — the interpreter and
every inner tool it spawned are in the executor's process group and fall to
the one SIGTERM) and the whole-tree `max_wall_seconds` budget (ARCH §6). A
per-tool deadline would be a third home for one fact, and a program is not a
new way to hang: `sleep infinity` already existed. Stated so nobody rediscovers
it: a budget is checked at model-call boundaries, so a program that never
returns is ended by `stop`, not by the budget.

### 2.6 Python availability is a grant, not a probe

Nothing declares or probes python3. `bash` assumes `sh` and says so in its
definition; `python` assumes `python3` and says so in its. An operator who
lacks it does not grant `python` to the role — the role's `tools:` list is
already the one home for "this deployment offers this tool", and a second
home (a probe at `litany new`, a capability flag) would drift from it. Under
yog the interpreter runs on the engine's box (§2.8), so the fact is the
server operator's, and the grant is where a server operator already speaks.

### 2.7 The stub module is regenerated per invocation

Generated fresh for every `python` invocation from the effective toolset the
grant gate reads at that step (`Resolved.grant.tools` plus
`dispatch::tools::injected`) — the same resolution, read at the same moment, so
the module cannot offer a function the door would refuse, and a followed tip
that changes the role's grant (bl-37cd's concern) is reflected at the next
program. Per dispatch would be a snapshot of a fact that moves. The cost is one
small file write per program, out of the worktree.

Shape: one keyword-only function per tool, parameters from the schema's
`properties` (required ones without defaults), docstring from the tool's
description, returning a `Result` with `stdout`, `stderr`, `exit_code` and
`ok`; it never raises on a non-zero exit (a program filtering failures wants
the code) and raises only when the door itself cannot be reached. `python` is
absent from its own module and the verb declines it — depth 1, the multi-tool's
rule, for the multi-tool's reason.

*One general path under the sugar (bl-0009).* The module's functions are
wrappers over one public `invoke(name, arguments)`, which is what a name
python cannot spell is reached by — a routed host tool named with a
hyphen, say. The alternative was renaming such a tool inside the module,
which would be a second spelling of a name the toolset owns, or dropping
it silently, which would hide a permitted tool from the one consumer that
could use it. Nothing is decided in the module either way: `invoke`
reaches the same door under the same gates, and a name the grant does not
carry is declined there. The same rule governs a property the schema
names but python cannot spell: it is absent from the signature, present
through `invoke`, and never renamed.

### 2.8 Authority and placement

`python` is an in-process built-in and therefore in the trusted computing base
(ARCH §3.6: "Shipping a tool in-process is the decision to place it in the
trusted computing base"); the program runs with the engine process's host
authority, as `bash` does today, and the v1.1 sandbox governs neither. What
the door adds is that every tool the program *composes* is adjudicated — the
control sees the outer `{name: python, input: {program}}` and can read the
program, then sees each inner invocation under its derived id.

Under yog, `python` is an **engine act**, never routed to a foot: its subject
is the agent's in-flight step — the record directory the stub is served from
and the inner records land in — which lives with the engine; the worktree the
program reads is the engine's server-side copy (yog `docs/REMOTE.md` §5.4).
Its inner invocations route per their own subjects, so a program running
beside the engine composes tools that execute on a foot. The engine-act set is
yog's derivation (`src/tool_host/engine_act.rs`) and gains the name there.

## 3. B — the standard corpus, one verdict per row

Every built-in is context spent on every step, so the test for ADD is "a
strong model needs it *and* neither a shell line nor a program is the same
path." Under that test the corpus adds `python` and nothing else.

| Row | Verdict | Reasoning |
|---|---|---|
| read_file offset/limit | HAVE via `bash` | `sed -n 'A,Bp'`; the >1 MiB decline already names `head`. Two optional parameters would be a second home for a range `sed` already expresses. |
| write whole file | HAVE via `apply_patch` / program | `*** Add File:` is the typed path; a program's `open(p, "w").write(...)` is the blob path. A `write_file` would be a third. |
| glob / find | HAVE via `bash` | `find`, `rg --files`, shell globs. |
| grep | HAVE via `bash` | `grep -rn`, `rg`. A grep tool would restate a tool every model was trained on. |
| list directory | HAVE via `bash` | `ls -la`, `find -maxdepth 1`. |
| bash timeout | HAVE via `bash` | `timeout 30 cmd` (coreutils). The tool definition gains the sentence. |
| bash cwd | HAVE | The `cd` built-in (ARCH §3.3 *one mutable per-agent fact*). |
| bash background | REFUSE | Long separable work is a `dispatch` (PRINCIPLES *Symmetry of dispatch*); a detached process outliving the invocation writes off the record and past the commit. |
| web fetch | HAVE via program / `bash` | `urllib.request`, `curl`. HTML-to-markdown is a library the harness will not carry. |
| web search | REFUSE in core | Needs a credentialed provider — an external `litany-tool-web_search` per deployment (PRINCIPLES *Integrations are external binaries*; `docs/DESIGN_MCP_BRIDGE.md`). The harness never holds a credential. |
| child status / poll | REFUSE | Parking is the poll: "the handle/`await`/`check` machinery is deleted, because parking-plus-revival already does its whole job" (PRINCIPLES). |
| model-callable stop | REFUSE, shape stated | `message` the child — every sender steers alike (ARCH §2.11). A stop is a grant of authority over another agent; if a workflow shows a parent needing it, it is one built-in re-entering `litany stop` restricted to the caller's own descent (`<id>-` prefix, ARCH §2.9), filed then. |
| todo / plan list | REFUSE | A plan is a file the agent writes with `apply_patch`; whether it composes is the manifest's (PRINCIPLES *Context has one home*, *File path as hint*). |
| ask user, blocking | HAVE | `message` to the user, then quiescence: "a parent waiting on a child is parked exactly as a root agent is parked waiting on the user" (PRINCIPLES). The block is the park. |
| notify | HAVE | `message` to the user. `notify_ui` stays a workflow action (ARCH §6). |
| schedule | REFUSE | The harness holds no clock daemon (PRINCIPLES *Regenerability*); a schedule is an operator's cron running `litany message` or `litany prompt`. |
| run code composing tools | ADD: `python` | §2. |

Two definitions change text, not shape: `bash` gains the `timeout` sentence;
`read_file`'s decline already points at `head`. The template `worker` grant
(`template/providers.yaml`) gains `python` and loses `multi_tool` (§5).

## 4. C — catalog capping

**No cap mechanism in the engine.** Three reasons, in strength order.

1. **A relevance-trimmed manifest is not a function of the tree.** ARCH §5.1:
   what the model sees is a pure function of the read-state commit; a BM25 or
   regex rung reads a query and an index, and its answer would differ across
   replay. `docs/DESIGN_MCP_BRIDGE.md` §7 refused live discovery for this
   reason and it stands.
2. **Schema-on-demand has no wire home.** A provider requires a tool's
   `input_schema` in the array before the model may name it (ARCH §3.3, *the
   array is closed over the history it ships*). A `describe` tool that
   returned a schema would leave the model naming a tool the array omits, and
   the provider refuses. The only lawful on-demand load of a *tool* is one that
   rebuilds the array — a paid prefix flush at a moment the agent chose.
3. **That rung already exists, at the host, and it is the skills' rule.**
   Skills: description-always, body on `load_skill` (a body insert that pays
   its flush). Tools under yog: `clients get` describes, `load` rebuilds the
   array once, at the step after (`docs/REMOTE.md` §5: "nothing but an
   explicit load ever changes the tool surface"). **One rule for both:
   advertise cheaply, elect to pay the flush.** A deployment with a large
   catalog pins per role (bridge §7) or loads per agent (yog §5); neither
   needs the engine to count tokens, and the engine has no tokenizer to count
   with (ARCH §3.3, bl-d5fa).

The program is the other half of the answer: many narrow tools in a large
catalog are one-line programs over a small corpus, which is why §3 adds one
built-in and not twelve.

*Deferred, not filed.* Anthropic's `defer_loading` tool search
(`docs/TAXONOMY.md` §4) is a server-side tool and the one cache-safe form of
a searchable catalog — the provider searches inside its own prefix. Server
tools are unwired (ARCH §2.10); when they land, that is a brazen row and an
assembly flag, not an engine mechanism. Compaction, memory and nested context
files are another identity's design and are not touched here.

## 5. D — skills and tools the program makes redundant

Every skill in `skills/` is a tool's skill; there is no standalone skill to
retire. The test for a tool is: **is it a composition of the others?** One is.

- **`multi_tool` retires.** A program is a multi-tool envelope whose list is
  written at run time, with the sequencing (`on_failure`) and the fan
  (`parallel` — a thread pool over `litany_tools`) as ordinary control flow.
  Keeping both is two paths to one round-trip saving (PRINCIPLES *One obvious
  path*). What moves, not dies: `multi/inner.rs` becomes the door verb's body
  (§2.1); the envelope parser, `multi/parallel.rs`, the schema and the skill
  are deleted, and the `bl-8ee7` paragraphs in ARCH §3.3 fold into the
  program's. The taxonomy's *multi-tool envelope* entry retires with it;
  *inner invocation* stays, widened (§1).

  **Shipped (bl-99bb), with one deletion this list did not name.** The
  executor's batch API — `ToolExecutor::execute_all`, `SpawnTool`'s threaded
  override and the `spawn_fan` / `route_fan` backends — existed for exactly
  one caller, the `parallel` envelope, and had none once it went; a
  fan is a program's thread pool now, and the harness classifies no tool as
  safe to overlap. `SpawnTool`'s prepare / answer / land split stays, because
  what it buys is keeping the clock, the git runner and the PATH lookup off
  any thread that blocks. `tool_step` now answers **no** tool name itself:
  every invocation it clears goes to the executor, and the door's depth-1 list
  is one name. The history rule holds and is pinned in both directions: a
  transcript that names `multi_tool` still assembles, with the stand-in
  `{"type": "object"}` schema the closure gives any undescribed name
  (`prompt::dispatch::tools::tests_retired`), and a fresh `multi_tool`
  invocation is declined in band as the ungranted name it now is
  (`cmd::tests::invoking_gates`).
- **Everything else stays.** `bash` is cheaper than a program for one command
  and needs no interpreter; `read_file` and `apply_patch` are tuned shapes
  with typed declines; `cd`, `dispatch`, `message`, `load_skill` act on
  harness state a program cannot reach except through them.

The retirement lands *after* `python` (§6 order), so the template never
carries a step with neither.

## 6. Implementation order

Each ball names its section, the files it touches and the proving test.

1. bl-e8d7 — `LITANY_TOOL_ID` on the stdio contract (§2.3): executor and
   ARCH §3.3.
2. bl-bae9 — `litany invoke` (§2.1): the verb, from `multi/inner.rs`.
3. bl-0009 — the `python` built-in and its stub module (§2.2, §2.7): triple,
   `NAMES`, toolspec pins, template grant — needs 1, 2. **Shipped** (ARCH
   §3.3 *The program*): `src/prompt/tool/builtin/python/`, with the door's
   caller resolution shared out to `dispatch/door/caller.rs` — one
   resolution, two surfaces — and the spawned-child cascade shared with
   `bash` in `builtin/child.rs`.
4. bl-99bb — retire `multi_tool` (§5) — needs 3. **Shipped**; see §5 for
   the one deletion beyond the list (the executor's batch API).
5. yog bl-fe43 — `python` joins the engine-act set; the re-exec target answers
   `invoke`; routed spawns carry `LITANY_TOOL_ID` (§2.2, §2.8) — needs 1, 2
   published and pinned.

## 7. Refusals, restated

- No in-process RPC between interpreter and window — §2.1, the front door.
- No per-invocation deadline — §2.5, one fact one home.
- No python probe or capability flag — §2.6, the grant is the declaration.
- No new file/search/list/write built-ins — §3, a shell line is the path.
- No manifest cap, no `describe` rung, no tool search in the engine — §4.
- No blocking ask, no poll, no schedule, no background — §3, parking and
  dispatch already are those.
