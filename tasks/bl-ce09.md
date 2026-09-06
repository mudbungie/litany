+++
title = "the shipped tool_output bound is 32 KiB per stream, so three --help calls fill a context and an ordinary conversation compacts six times"
created = 1788673662
updated = 1788674852
claimant = "Cantaloups-L2"
priority = 2
root_commit = "12899370c9ec7a5ed7f8e26d3d4fb914ea6c3310"
tags = ["usability-r1"]
+++
Round-1 daily lane. litany 0.0.10 under yog 0.0.38, seat lernie 0.1.30, on the shipped `template/workflow.yaml`.

Two shipped numbers meet badly, and the result is that ordinary conversations compact several times and cost millions of tokens.

    tool_output:
      head_bytes: 16384
      tail_bytes: 16384

    compaction:
      intermediate:
        trigger: every_n_commits
        n: 20

32 KiB per stream is roughly 8,000 tokens for ONE tool result. Three ordinary calls fill a context that a conversation then has to compact. Measured, on the goal "write a status report of the open balls in this project" against a four-ball store:

- `bl --help; bl list --help` — 7,684 bytes into the transcript, whole.
- a `find` the model ran over the home tree — 32,985 bytes into the transcript, essentially whole (it stopped just under the cap).
- six compactor children over four minutes, cumulative spend 2,607,869 tokens, for a report about four tasks.

A second conversation on the same box, reading a repository, reached 122,000 prompt tokens by its eighth step purely on `cat` output and compacted three times on the way to 4,060,308 tokens.

The commit clock is the other half. `every_n_commits: 20` counts commits, and a step writes two, so a compactor fires about every ten steps whatever the context actually holds — the template's own comment explains why `window_percent` is not the default (only one of brazen's rows can state a context window), which is a fair reason to ship the commit clock and not a reason for the head/tail cap to be this large underneath it.

For comparison, on the same three goals in this lane, `claude -p` and `codex exec` both truncate a shell capture far more aggressively and neither compacted at all; codex finished the same status-report goal on 51,733 tokens total.

Expected: the shipped bound is small enough that a `--help` or an `ls -la` is a few hundred tokens, with the marker already implemented ("the omitted middle is replaced by a marker stating the original byte/line counts and where the full record lives") doing the work. The full capture is on disk and one query away; the transcript does not need it. 2 KiB head + 2 KiB tail would have kept every tool result in this lane intelligible and compacted nothing.

Severity p2: it is a config default rather than a defect, and it is the single largest cost multiplier this lane measured. It is severable — the ruling is one number in one file.