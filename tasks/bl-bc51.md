+++
title = "the shipped workflows/learning-loop.yaml tells the operator stage_proposal is unimplemented vocabulary, and it has been implemented since bl-5b62"
created = 1788673910
updated = 1788673910
priority = 3
root_commit = "12899370c9ec7a5ed7f8e26d3d4fb914ea6c3310"
tags = ["usability-r1"]
+++
Scenario: lane KNOWLEDGE, round 1. Before adopting the learning loop I read the seeded template, `<config-root>/workflows/learning-loop.yaml`, which `litany prime` writes. Its comment above the binding says:

    # The reviewer's landing (§3 there): one config commit on
    # `proposal/<reviewer-id>`, parented on the followed config commit the
    # reviewer read, which no lineage points at until `litany proposal
    # --accept` fast-forwards it. Vocabulary today — the action parses and
    # the interpreter declines it until its landing ships.

That last sentence is false. `src/prompt/dispatch/child_result/proposal.rs` implements it, `child_result.rs:243` dispatches `Action::StageProposal if landing::qualifies(cr)`, and I drove the whole thing end to end in this lane: three proposals staged on `proposal/<reviewer-id>`, `litany proposal <ws> <id> --accept` fast-forwarded the lineage, `--reject` deleted the other two, and the next conversation was born with the accepted skill patch and used it.

WHY IT MATTERS

This is the file an operator reads to decide whether to adopt the feature. The comment tells them the feature does nothing yet, so they do not turn it on — the one document whose job is to make adoption a config edit instead argues against it. The same template is the answer to "how do I get the learning loop", so the comment is load-bearing.

EXPECTED

Drop the sentence, or replace it with the shipped-state note the codebase uses elsewhere naming the ball that landed it.

SEVERITY

p3: a doc defect, but in the one place that gates use of a working feature.