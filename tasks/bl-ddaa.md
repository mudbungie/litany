+++
title = "the injection seam withholds the two facts its consumers own: tools() cannot name the driven agent and route() cannot name the caller's cwd"
created = 1788150785
updated = 1788150786
priority = 9
root_commit = "12899370c9ec7a5ed7f8e26d3d4fb914ea6c3310"

[[blockers]]
id = "bl-d3b8"
on = "close"

[[blockers]]
id = "bl-fba0"
on = "close"
+++
Two downstream defects (yog bl-fd24 and bl-77be, evidence there) trace to one
seam shape: docs/DESIGN_TOOL_INJECTION.md declares the injection per-process
and hands the declaration half nothing per call.

**Half one: `tools()` takes no context, so a linked binding reads the driven
agent off its own argv.** A `prompt` driver mints its agent, argv names none,
and the host's injection declares nothing for the whole drive — the first
driver of every conversation can `load` a remote tool (the load answers ok,
"callable from the next step on") and can never call one, because the grant
gate enumerates exactly what `tools()` returned. The composer knows precisely
which agent each request is assembled for (`repo`/`conv_id` at all four
`dispatch::tools::injected` call sites); the binding does not. Fix where the
fact is born: `ToolInjection::tools(&self, workspace: &Path, agent: &str)`,
`ToolExecutor::injected` likewise, and the four call sites hand it over.
This re-draws one sentence of §7 ("litany will not do it for them"): the
discriminants RoutedCall already carries reach the declaration half too. The
seam stays per-process; what changes is that a declaration is asked FOR an
agent, which it always was in fact.

**Half two: `RoutedCall` omits the caller's resolved cwd, so a router cannot
say where the subject lives.** `prepare` already resolves it for every call
(`Caller::resolve`: the cwd mark if live, else the worktree — ARCH §3.3
*Working directory*) and hands it to the spawning backend; the routing backend
is told only the workspace root. A host routing a worktree-subject tool to a
remote executor (yog REMOTE §5: "an invocation carries its subject's
location") must either re-derive the mark — a second home for this crate's own
fact, reading a ref namespace §3.3 keeps consumers out of — or be handed the
value the executor already computed. §3.4's rejection of cwd on the seam was
priced against a *spawning* backend's needs (cwd AND env); the env half stays
out, the location half is now a consumer's own requirement. Add
`RoutedCall::cwd`.

Both halves are one release: the seam is pin-exact 0.x with no stability
promise, and the one linked binding moves in step.