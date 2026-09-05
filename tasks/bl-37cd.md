+++
title = "a followed tip that changes a role's grant leaves the fork-time descriptions cut stale in the agent's tree"
created = 1788243980
updated = 1788585904
claimant = "Animations-AA"
root_commit = "12899370c9ec7a5ed7f8e26d3d4fb914ea6c3310"
+++
Under bl-403b (follow-the-tip resolution, operator ruling 2026-09-01) an agent's grant follows its lineage's current head, but descriptions/** in its tree are the dispatch-time cut (§3.3 — they are committed context, not control). A tool the new tip grants resolves but describes nothing in context; a tool the new tip revokes stays described. Decide and build the refresh: re-derive the cut at the step boundary when the followed commit changes, or an explicit refresh act — without breaking §5.5 append-only assembly more than a compaction already does.