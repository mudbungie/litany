+++
title = "compactor and reviewer burn a model call per refused bash: 360 refusals across 33 reviewer branches in one lane"
created = 1788673904
updated = 1788673904
priority = 3
root_commit = "12899370c9ec7a5ed7f8e26d3d4fb914ea6c3310"
tags = ["usability-r1"]
+++
Scenario: lane KNOWLEDGE, round 1. Both checkpoint roles fork with the dispatching branch's transcript inherited, which declares the worker's tools. The first thing each one reaches for is `bash`, and it is refused:

    "bash" is not callable by a compactor: it is declared only because the
    inherited transcript references it. The compactor toolset is
    write_summary, mark_for_deletion, clients (ARCH §3.3, declaring is not
    permitting).

    "bash" is not callable by a reviewer: it is declared only because the
    inherited transcript references it. The reviewer toolset is apply_patch,
    read_file, clients (ARCH §3.3, declaring is not permitting).

COUNTED

Across the 33 reviewer branches of one lane, tool calls by name:

    bash 360, read_file 329, clients 77, apply_patch 9

360 refused `bash` calls. Each one is a full model round trip whose prompt is the entire inherited transcript. The compactors did the same thing at their first step, every time.

The refusal message is correct and even explains itself; the model tries anyway, repeatedly, because the tool is in the declared list it can see.

EXPECTED

The declaration exists so an inherited transcript still assembles (bl-5a1f). It does not have to be offered as a callable definition on the wire. Either mark it declined in the definition the model sees (an `input_schema` the model cannot satisfy, or a description that leads with "not callable by this role"), or move the refusal earlier so it does not cost a round trip.

SEVERITY

p3: pure waste, no wrong answer. It is p3 rather than p4 because it scales with the number of checkpoint dispatches, which is exactly what the learning loop multiplies.