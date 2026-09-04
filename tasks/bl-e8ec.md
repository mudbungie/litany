+++
title = "context economy design: durable facts, history search, usage-triggered compaction with a token tail and a deterministic extract, nested context files, recoverability — docs/DESIGN_CONTEXT_ECONOMY.md"
created = 1788493049
updated = 1788493049
claimant = "Thrift"
parent = "bl-8175"
priority = 1
root_commit = "12899370c9ec7a5ed7f8e26d3d4fb914ea6c3310"
+++
Deliverable: docs/DESIGN_CONTEXT_ECONOMY.md, a living design document answering six questions with a decision and its reasoning each, plus amendments to ARCHITECTURE/PRINCIPLES/TAXONOMY where a decision changes an invariant. Implementation balls are filed per severable piece as siblings under bl-8175, gated on this one. Ruling driving it: context is the scarce resource; the engine is conservative with it. A: durable facts as a capped config-lineage file cut into the pinned head at every fork. B: history search over the workspace's own agent refs (the compactor's ref is the soft archive), no second store. C: a usage-based checkpoint trigger, a token-budget retained tail, a code-written extract beside the model summary as a second compaction product, soft archive stated. D: context files discovered from the agent's cwd and carried on the next tool result, never the pinned prefix. E: the bounded projection meets the lesson; gaps stated. F: rollback-preserving-later-edits verb refused, the existing primitives named.