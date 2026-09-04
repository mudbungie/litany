+++
title = "keep_recent_tokens: the retained tail as a provider-reported prompt-token budget, the compaction point derived from successive usage reports"
created = 1788493095
updated = 1788493095
parent = "bl-8175"
priority = 2
root_commit = "12899370c9ec7a5ed7f8e26d3d4fb914ea6c3310"

[[blockers]]
id = "bl-e8ec"
on = "claim"
+++
Implements docs/DESIGN_CONTEXT_ECONOMY.md section 5.2 (C.ii, the token tail).
- src/config/workflow/compaction.rs: CompactionConfig gains keep_recent_tokens: Option<u32>; declaring it beside keep_recent is declined at parse naming both keys.
- src/prompt/dispatch/child_result/flush.rs (where the compaction point is derived from keep_recent today): under keep_recent_tokens the point is the newest model-entry commit whose usage prompt side is at most n below the tip's — walk the branch's model entries newest first (git log over messages/*-<model-id>.json is the derivation; no stored counter), and the point is a step boundary by construction. A branch with fewer tokens than n in its whole history has its point at the checkpoint origin, i.e. nothing to compact — not due.
- template/workflow.yaml: keep_recent_tokens: 20000 shipped beside the trigger; the comment says one or the other.
Tests: src/config/tests/workflow_compaction.rs (both keys declined), flush.rs tests (point lands on the entry whose delta crosses n; absent usage on an entry contributes nothing; a tail shorter than n yields no point). Docs: ARCH section 6 already amended; TAXONOMY retained tail entry names the token form. Gates: tests 100 percent, docs, alignment.