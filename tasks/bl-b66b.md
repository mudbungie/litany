+++
title = "context files: AGENTS.md/CLAUDE.md on the cwd's path, not yet shown to this agent, appended to the next tool result — a workflow.yaml context_files list"
created = 1788493096
updated = 1788493096
parent = "bl-8175"
priority = 2
root_commit = "12899370c9ec7a5ed7f8e26d3d4fb914ea6c3310"

[[blockers]]
id = "bl-e8ec"
on = "claim"
+++
Implements docs/DESIGN_CONTEXT_ECONOMY.md section 6 (D, context files).
- New src/config/context_files.rs: the context_files: [names] block, wired in src/config/workflow.rs beside tool_output; absent = nothing discovered. template/workflow.yaml ships [AGENTS.md, CLAUDE.md].
- src/prompt/dispatch/tool_step.rs (and settle.rs) at the point the tool result entry is rendered: resolve the agent's cwd (the existing prompt::tool::spawn::Caller::resolve mark read), compute the path set — git rev-parse --show-toplevel of the cwd down to the cwd when inside a repository, else the cwd alone — and for each name present on that path that no tool entry in the read-state tree already frames (a query over messages/NNN-tool.json for a <file path=...> frame with that absolute path; no mark written), append the file after the envelope, framed like ARCH 5.3 and bounded by the tool_output policy as its own stream. src/prompt/tool/envelope.rs gains the appended-file rendering.
- The pinned head is untouched; the append is a tail append (cache-safe).
Tests: src/prompt/tool/tests/moved_cwd.rs (after cd into a subdirectory carrying AGENTS.md the cd result carries it; the next result does not; a --cwd seeded agent's first tool result carries it; a repository's top-level file plus the subdirectory's both ride, top first; a name absent from the list is never read; after a compaction that removed the carrying entry it is shown again), envelope tests for the frame and the bound. Docs: ARCH 3.3 and 6 already amended by bl-e8ec; the shipped-state note names the foot limitation. Gates: tests 100 percent, docs, alignment.