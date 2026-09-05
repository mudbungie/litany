+++
title = "compare prints each side's Wilson interval but no verdict on the delta: add a paired per-task significance answer for the pass@1 difference"
created = 1788317887
updated = 1788586740
claimant = "Animations-AA"
root_commit = "12899370c9ec7a5ed7f8e26d3d4fb914ea6c3310"
+++
The v1.0 criterion (ARCH §12) is a variant beating baseline by a statistically significant margin on pass@1, but agent-eval compare (bl-36fa, bl-f838) renders the delta with each side's own interval only — no answer about the difference. Runs cluster within tasks, so a pooled two-proportion test over all runs would be dishonest; the honest shape is paired per task over the shared set (sign test or permutation over per-task pass-rate differences, or a bootstrap). Deliverable: one named method rendered in compare's total block beside its assumptions. Never a model judging, never new instrumentation — the saved records already carry every per-run outcome.