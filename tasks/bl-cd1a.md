+++
title = "Integrate scotoma: secret handles substituted at execution, values redacted from output and requests"
created = 1794986620
updated = 1794986620
root_commit = "12899370c9ec7a5ed7f8e26d3d4fb914ea6c3310"
+++
scotoma 0.0.1 is on crates.io (repo: ~/dev/scotoma; contract: its docs/DESIGN.md 'Integrating a harness'). Zero dependencies. API: Scotoma::open(home, actor), issue(name) -> uuid handle, substitute(&str) -> String, redact(&str) -> String, plus streaming substituter()/redactor(). Handles are read from scotoma's on-disk usage log, so they survive the per-tool subprocess and a hold resuming in a new process.

Intended flow in one assistant turn: tool scotoma {name} returns a handle; a later bash call carries the bare handle in its command; the value is swapped in just before execution.

Proposed seams (verify against code before building):
1. New tool scotoma: input {name}, result = the handle.
2. Substitute in SpawnTool::prepare (src/prompt/tool/spawn/batch.rs), AFTER input.json is written in handle form and after the seam adjudicated the handle form; substitute inside each string of the input Value, never over serialized JSON.
3. Redact captured stdout/stderr in SpawnTool::land BEFORE bound::apply and BEFORE output.json (read_tool_output pages that file).
4. Redact the serialized request in one_call::issue before request.json and the bz stdin (the redactor also matches a value's JSON-string form).

Open decisions for the operator:
- Under yog's injection, bash is routed to a remote thrall. Substituting in litany sends the value through yog's gesture files and mailbox; substituting in thrall needs a vault on the executing machine.
- Where the vault lives (SCOTOMA_HOME) and what the actor is (agent id?).
- python -> litany invoke is another process and passes through the same SpawnTool, so it is covered by 2-3; confirm.