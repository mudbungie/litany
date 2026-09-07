+++
title = "a cut tool result is a permanent loss the harness could recover: mint a continuation address into the agent's own capture and add read_tool_output to page it"
created = 1788752240
updated = 1788752241
claimant = "Cantaloups-L8"
priority = 1
root_commit = "12899370c9ec7a5ed7f8e26d3d4fb914ea6c3310"
tags = ["usability-r3"]
+++
## The gap

The §3.3 **bounded transcript projection** cuts each captured stream to
`head_bytes + tail_bytes` (shipped 2048+2048, `docs/DESIGN_CONTEXT_ECONOMY.md`
§7.1) and states in the marker where the full bytes live —
`steps/<agent-id>/<NNN>/tools/<tool-id>/output.json`, outside every worktree.
§7 names the residual outright:

> The marker's recovery path is `steps/…/output.json`, outside the worktree:
> reachable by `bash` on the engine's box, unreachable from a foot. §4's
> address is the model-followable pointer for *transcript* content; the raw
> capture stays diagnostic (ARCH §2.3), and "re-run with a filter" — the
> marker's own advice — remains the answer for it.

So the cut middle of a 30 KiB result is lost to the model, and the advice is to
run the tool again — for output that §3.3 itself calls nondeterministic ("Tool
outputs are nondeterministic and cannot be reconstructed by re-running, which
is exactly why their context home must be a committed file"). The one place the
bytes certainly still are is the capture the harness already wrote.

## The contract

**The raw capture becomes the one readable page store, and nothing else
changes.** No index, no database, no second file, no new directory: the address
IS the record path plus an offset, and the file it names is the `output.json`
the executor already writes.

### 1. The continuation address

A **continuation address** is a workspace-relative record path with a stream and
a byte offset appended:

    steps/<agent-id>/<NNN>/tools/<tool-id>/output.json#<stream>@<offset>

`<stream>` is `stdout` or `stderr`; `<offset>` is a byte offset into that
stream as the record stores it. Nothing is stored to mint one and nothing is
stored to redeem one — every component is already a fact of the call, and the
grammar is the whole parser.

**Why "address" and not "pagination token".** Two reasons, both hard. In this
tree a *token* is a model token — §3.3 states counts in bytes precisely because
"litany has no tokenizer, and a fabricated token count would be a lie in the
transcript" — so naming a path a token collides with the one word the context
economy is measured in. And the tree cannot hold the other name anyway: the
disclosure gate's `credential-assignment` rule refuses any committed
`token: "<8+ chars>"`, which is exactly what the tool's input schema, its
`SKILL.md` example and the cut marker would all have spelled. `address` is
already the corpus's word for a followable pointer (`search_history`'s
`<commit>:<path>`), so this is the ladder's own vocabulary, not a dodge.

**The agent id in the address is the domain bound, structurally.** The tool
resolves the address against `LITANY_CONV_REPO` and refuses when the address's
`<agent-id>` segment is not the caller's own `LITANY_CONV_BRANCH` — refused *by
name*, stating both ids, so an agent cannot read another agent's captures and
cannot discover that one exists. There is no allowlist and no check to keep in
step: one string equality on a component the address cannot omit. The strict
grammar is also the traversal guard — six segments, the literals `steps`,
`tools` and `output.json` fixed, `<NNN>` all digits — so no `..` and no
absolute path can be spelled at all.

### 2. The tool: `read_tool_output`

A built-in ([`builtin::read_tool_output`]), in the corpus's verb_noun voice
(`read_file`, `load_skill`, `search_history`) — **not** `tool_output`, which is
already the name of the `workflow.yaml` policy block that does the cutting, and
one word for two things is how a terminology ladder rots.

Input: `{"address": "<address>"}`, nothing else — the address carries every
discriminant, so a `stream:` or `offset:` field beside it would be a second
home for a fact the address already states.

Output, on the §3.3 stdio contract:

- **stdout** — the page: the stream's bytes from `<offset>`, `PAGE_BYTES` of
  them (4096 — the shipped allowance of §7.1, so a page rides the projection
  that made it untouched), or the remainder when less is left.
- **stderr** — the banner, in `read_file`'s shipped voice (§7.1: *"read_file:
  200 of 843 lines from offset 1; continue with offset 201"*), naming the byte
  window, the stream's total, and either the next address or the end of stream.

The banner rides stderr and not stdout for the reason the size is what it is:
the executor bounds each stream independently, so a page that is exactly the
allowance passes through unbounded while its banner is bounded as its own tiny
stream. A page cut in half by the very projection it exists to answer would be
a joke; this makes that structurally impossible under the shipped numbers, and
under a *narrower* operator policy the ordinary projection applies to the page
as to any other result — which is what a policy is for.

Declines (each an `is_error` result the model reads verbatim): an address that
does not parse, naming the grammar; a foreign agent id, naming both; a record
that is not there (retention, §9.2); an offset past the end of the stream.

### 3. The cut message states the address and the tool

`bound::apply`'s marker gains the continuation, so the model never has to know
the grammar:

    [... stdout truncated: 31337 bytes / 900 lines total; showing the first
    2048 and last 2048 bytes; full record: steps/a/007/tools/tu_1/output.json;
    read the cut middle with read_tool_output
    {"address": "steps/a/007/tools/tu_1/output.json#stdout@2048"} ...]

The offset is `head_bytes` — the first byte the model was not shown.

`bound::apply` has two other callers whose cut bytes are **not** a tool capture
and have no address: a context file (whose full copy is the file itself, at the
path the marker already names) and a `search_history` preview (whose full copy
is the entry, at the address the marker already names). The 4th parameter
therefore stops being a bare path and becomes `Origin::{Capture, Named}` — a
capture mints an address, a name does not. That is one enum, not a flag: the two
markers say different true things.

### 4. The doc amendment, deliberately

`docs/ARCHITECTURE.md` §2.3's *Diagnostic-only contract* says of the per-tool
records "**never read at runtime**", and §5.2's *Shipped-state note* says
`output.json` is "still *written* as the diagnostic raw capture, never read".
Both become false in exactly one place and must say so rather than be left to
rot:

> `tools/<tool-id>/output.json` is read at runtime in exactly one place: the
> `read_tool_output` built-in, answering a continuation address the calling agent
> was handed, for a capture under that agent's own id. Nothing about the
> diagnostic-only contract weakens elsewhere — `input.json`, `request.json`,
> `response.json`, `stderr.log` and `driver.log` are read by nobody, no
> *decision* anywhere turns on `output.json`'s content, and **no assembly path
> touches `steps/` at all**: the read is a tool call an agent makes, landing in
> the transcript as an ordinary tool result, exactly as a `read_file` of a work
> product does. The record is still not context; it is now addressable.

The one honest seam, stated where it lives: the record stores each stream as
lossy UTF-8 (§3.3 *Disk record*), while the projection cuts raw bytes, so for a
stream that is not valid UTF-8 an offset into the record is not exactly an
offset into the capture. For every ordinary case they coincide, and the record
is the store — the address's offsets are offsets into it.

`docs/DESIGN_CONTEXT_ECONOMY.md` §7's second bullet is the gap this closes and
is rewritten to say so; §7.1's "re-reading a named range costs one cheap tool
call" now holds for a *tool result* as well as for a file.

## Verification

- A cut result carries an address: the marker names `read_tool_output` and a
  well-formed address whose offset is `head_bytes`.
- Paging returns the remainder and ends: consecutive addresses walk the stream to
  its end and the last banner says so; the concatenated pages equal the
  capture's tail from the first offset.
- A foreign-agent address is refused by name, and so are the ungrammatical, the
  absent record and the past-the-end offset.
- **Nothing under `steps/` is read on the assembly path**: after a cut, the
  whole `steps/` tree is deleted and assembly still composes the same wire
  history, marker and address included — the address lives in the committed
  transcript, never in the store it points at.
- The `bash` / `read_file` / context-file / `search_history` markers are
  unchanged where no address exists.

## Scope

`src/prompt/tool/**` and the recording path only. The name joins `NAMES`, so
the shipped **worker** grant gains it (the shipped rule: the worker's grant is
the whole pool). Every other role's grant is an operator's config edit and is
untouched here — a confined role's cut marker names the record path exactly as
it does today.