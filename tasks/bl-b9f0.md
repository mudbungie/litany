+++
title = "a compaction in flight does not suppress the next checkpoint, so a second compactor is dispatched off the same span and both write summary/001.md"
created = 1788150358
updated = 1788151149
priority = 7
root_commit = "12899370c9ec7a5ed7f8e26d3d4fb914ea6c3310"
tags = ["compaction"]

[[blockers]]
id = "bl-34e9"
on = "close"

[[blockers]]
id = "bl-c815"
on = "close"
+++
`prompt/compactor/checkpoint.rs` evaluates the trigger at each step boundary
against a checkpoint reference that is "the newest of {its dispatch commit, its
last compaction base}". A compaction that has been DISPATCHED but has not yet
landed has written no base — so the next boundary computes the same span, sees
the same count, and fires again.

Observed on a live run with the stock `every_n_commits: n = 20`:

- boundary after step 010 -> compactor A dispatched
- boundary after step 011, eight seconds later -> compactor B dispatched

Both were dispatched off the same branch, both saw substantially the same span,
both ran a full model loop, and both wrote `summary/001.md` — the same path,
because each one's `next_seq` saw a directory in which the other's summary did
not exist yet. Only B's compaction base survives in the branch log; A's whole
run is spend that produced nothing.

## Why the existing invariants do not cover it

The module documents two eligibility invariants and says either one alone stops
the bl-a9eb runaway: the clock starts at the branch's founding commit, and a
compactor is never itself compaction-eligible. Both held here. Neither is about
this: they stop a compactor being *forked off a compactor* and stop a young
branch reading its parent's history as its own. This is the parent branch firing
the same checkpoint twice because the first firing's answer has not come back.

## The shape of it

"Since the last checkpoint" is derived from git and nothing is stored — which is
the right principle and is exactly why an in-flight compaction is invisible. The
thing that is missing is not stored state so much as a derivation that can see a
dispatch: a compactor child forked off this branch that has not yet landed is a
fact git already holds (its dispatch commit names its parent branch), and
`origin()` could take it as the checkpoint reference the same way a base is
taken. That keeps the single-source-of-truth property — nothing new written,
one more thing read.

The cost of leaving it: on a chatty branch every boundary past the threshold
dispatches another compactor until one lands, each one a full model loop at the
compactor role's model. Two were observed eight seconds apart on a twelve-step
conversation; nothing in the mechanism bounds it at two.

## Interaction with the sibling ball

The same pair of runs also showed a compactor accepting `mark_for_deletion` of
its own summary (filed separately). The two defects mask each other: here, B's
base superseding A's is what saved the branch from A's self-deletion. Fixing
either one alone leaves the other live, and fixing the duplicate dispatch first
makes the self-deletion reachable with nothing behind it.