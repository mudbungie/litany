+++
title = "search_history built-in: git log -S over the workspace's agents/* refs returning stored transcript entries, bounded, with an address that recovers one whole"
created = 1788493094
updated = 1788493094
parent = "bl-8175"
priority = 2
root_commit = "12899370c9ec7a5ed7f8e26d3d4fb914ea6c3310"

[[blockers]]
id = "bl-e8ec"
on = "claim"
+++
Implements docs/DESIGN_CONTEXT_ECONOMY.md section 4 (B, history search) and pins section 5.4 (the soft archive).
New: src/prompt/tool/builtin/search_history/{mod.rs,tests.rs}, schemas/tools/search_history.json, skills/search_history/SKILL.md; src/prompt/tool/builtin/mod.rs NAMES gains the name (the shipped worker grants the whole pool, template/providers.yaml, so no grant edit).
Contract: input is exactly one of {pattern} or {entry}. pattern runs git log --diff-filter=A -S<pattern> --format=%H --name-only over every agents/* ref of the workspace (LITANY_CONV_REPO, harness-derived), newest first, restricted to messages/ and summary/; output lists every hit as commit:path one per line, then the newest five entries verbatim (git show commit:path), each capped 4 KiB head + 4 KiB tail with the ARCH 3.3 marker naming the address. entry returns that one entry whole. Both inputs, or neither, or unknown fields: declined as is_error. A pattern with no hit is a clean empty listing.
Tests: unit tests over a fixture workspace with two roots and one compaction (the pre-compaction entry is found through the compactor's ref after the landing squashes it — the section 5.4 pin; a deleted-then-squashed entry is found once, not per ref; the cap marker names the address; the entry path recovers the whole entry; the input declines). The skill body documents the two shapes and that a squashed span lives on the compactor's ref. Docs: ARCH 3.3 tool table gains the row; the shipped-state note. Downstream, not this ball: yog's tool host classifies a new builtin into the worktree lane by derivation, so yog needs search_history added to its engine-act set or the search runs on the foot where no repository exists. Gates: tests 100 percent, docs, alignment.