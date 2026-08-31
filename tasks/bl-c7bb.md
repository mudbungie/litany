+++
title = "a compactor may mark its own just-written summary for deletion, and the landing then carries away the whole span it was compacting"
created = 1788150354
updated = 1788150716
priority = 8
root_commit = "12899370c9ec7a5ed7f8e26d3d4fb914ea6c3310"
tags = ["compaction"]

[[blockers]]
id = "bl-80af"
on = "close"
+++
`prompt/compactor/tools.rs::mark_for_deletion` declines exactly two nominations:
the branch's dispatch entry, and a path that does not exist. Nothing else. In
particular nothing stops a compactor nominating `summary/NNN.md` — including the
one it wrote seconds earlier in the same run, through the other half of its own
procedure pair.

Observed on a live run. One compactor's transcript, in order:

    write_summary  {"content": …}      -> {"status":"written","path":"summary/001.md"}
    mark_for_deletion {"path":"summary/001.md"} -> {"status":"marked","path":"summary/001.md"}

Both accepted. The compactor then reported it was done.

## Why this is not merely odd

The landing squashes the compaction span into a compaction base and replays the
live tail on top. The summary is the ONLY thing the span leaves behind — the
module's own docs say so: "the compaction landing admits only the summary and
the deletions (ARCH §2.6)". A landing that carries a `git rm` of its own summary
carries away the entire history it was dispatched to preserve, and leaves a base
commit holding nothing. The branch's uncompacted content is gone and there is
no summary of it.

In the observed run the damage did not land, for an unrelated reason: a second
compactor was dispatched off the same span moments later (filed separately) and
its base superseded this one. That is luck, not a guard.

## The guard the code already reaches for

The dispatch-entry decline exists because "a compactor *is* the compaction, not
a subject of one". The same sentence applies with more force to the summary the
compaction is producing: a compactor may prune the branch's history, and its own
output is not part of that history. The natural rule is that
`mark_for_deletion` declines any path the *current* compactor's `write_summary`
produced — the run knows that path, it returned it.

The wider rule is worth a moment's thought before implementing the narrow one:
superseding an EARLIER summary is legitimate (the module docs call it out — "a
summary it cannot see is a summary it destroys when it supersedes it"), so a
blanket `summary/**` refusal is wrong. The distinction is this run's own output
versus a prior run's, and that is state the run already holds.

## A second, smaller thing seen in the same pair of runs

The other compactor nominated a `messages/NNN-user.md` that did not exist on the
branch and got the existence decline, which worked exactly as documented —
in band, non-zero, naming the pathspec. No defect there; recorded only so a
fixer reading the same evidence does not chase it.