+++
title = "merge-release-pr reads mergeability once, and GitHub answers UNKNOWN while it computes: re-read, bounded"
created = 1788582042
updated = 1788582042
priority = 2
root_commit = "12899370c9ec7a5ed7f8e26d3d4fb914ea6c3310"
+++
Run 33920455560 — the run the 0.0.9 release-prep landing triggered, and the one whose whole point was to release — printed `PR 19  hold: GitHub says mergeable=UNKNOWN  [merge state UNKNOWN]` and merged nothing. Read again a minute later, the same pull request answered MERGEABLE. GitHub computes mergeability lazily, on demand, and answers UNKNOWN while it does, so a single read taken right after release-plz refreshed the branch can be UNKNOWN for reasons that have nothing to do with the pull request.

The reconciler recovered — the next push to main (an unrelated lane's) re-judged and merged, run 33920904362 — which is the designed behaviour and is why nothing broke. But it makes the header's promise false in the case that matters most: `release-plz.yml` now says landing `make promote-changelog` IS the release act and that nothing after it is touched by a hand, and here the release instead waited on whatever push happened by next. Had no other work landed, 0.0.9 would have sat.

Re-read the open set, bounded, while any pull request in it reports `mergeable: UNKNOWN` — a handful of attempts a few seconds apart, then hold as now. Fold the initial `gh pr list` into bl-0187's rule at the same time: it is a READ, so a transient failure there should hold rather than redden the run, and today it is the one read still under bare `set -e`.

Gates: docs (the job header states the re-read and why), alignment (the AUTO-MERGE section's reconciler paragraph names `mergeable: UNKNOWN` as a thing retried on the next wake-up — that stays true as the fallback, but the wake-up should not be the FIRST answer). No Rust changes.