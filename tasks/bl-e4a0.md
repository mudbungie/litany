+++
title = "no step record says which config commit a step resolved: record the followed commit in meta.json for audit and replay honesty"
created = 1788243981
updated = 1788585139
claimant = "Animations-AA"
root_commit = "12899370c9ec7a5ed7f8e26d3d4fb914ea6c3310"
+++
Follow-the-tip (bl-403b) makes "which config governed step N" a fact about when the step ran, no longer derivable from the branch alone. request.json captures the config EFFECTS (model id, soul, tools) but not the commit identity. Record the resolved config commit (and the workflow-source commit when a mark stands, bl-f928) in the step meta.json beside the existing `commit` field — diagnostic provenance, same class as request.json, not a control input.