+++
title = "facts file: cut facts.md from the followed config commit into the pinned head at every fork, cap it at the write, decline its nomination"
created = 1788493093
updated = 1788493093
parent = "bl-8175"
priority = 2
root_commit = "12899370c9ec7a5ed7f8e26d3d4fb914ea6c3310"

[[blockers]]
id = "bl-e8ec"
on = "claim"
+++
Implements docs/DESIGN_CONTEXT_ECONOMY.md section 3 (A, durable facts). Four edits in one module family plus the template:
- src/prompt/dispatch/step_commit.rs trim_to_context: facts.md joins the dispatch-time cut beside descriptions/** — read from the followed config commit, written into the new branch's tree at the dispatch commit, re-cut at every fork (a child never inherits its parent's copy). Absent in the config commit = absent in the tree, no error.
- src/template/authoring/mod.rs: the config-authoring pass declines a commit whose facts.md exceeds FACTS_MAX_BYTES = 4096, naming the size and the cap (the shape of read_file::MAX_BYTES; a constant, not a manifest key — the doc says why).
- src/prompt/compactor/tools.rs not_compaction_eligible: facts.md joins the class (a dispatch-written fact is not the branch's history).
- template/manifest.yaml: worker pins facts.md; compactor does not.
Tests: src/prompt/dispatch/step_commit/tests.rs (a config commit carrying facts.md yields a branch tree carrying it; a child dispatched from a parent whose tree carries a stale copy gets the followed commit's bytes; absence is a no-op), src/template/tests.rs (4097 bytes declined, 4096 accepted, message names both numbers), the compactor tools tests (nomination of facts.md declined in-band), assembler test that the pinned block composes as a path-framed head block. Docs: ARCH section 5.5 and 2.7 already amended by bl-e8ec; add the shipped-state note. Gates: tests 100 percent, docs, alignment.