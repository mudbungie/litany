+++
title = "the two self-healing Ingest exits are green: a refused fast-forward (main moved while the gate ran) and a release still in flight (no tag for main's version) print their line and end the run successfully with nothing landed"
created = 1795071707
updated = 1795071707
priority = 2
root_commit = "12899370c9ec7a5ed7f8e26d3d4fb914ea6c3310"
+++
Operator ruling: neither needs a hand and the next hourly run retries, so neither may open the alarm issue. The in-flight hold moves ahead of every write in scripts/ingest/ingest, so the tree is untouched and the workflow sees an empty diff. In ingest.yml only the refused fast-forward itself is forgiven: after a failed push the run fetches main and forgives only when main is no longer an ancestor of the branch; any other push failure, a red ci.yml, a script error or an unreadable index stay red. The branch is deleted either way.