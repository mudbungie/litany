+++
title = "release 0.0.13: the compaction sweep no longer splits a tool window, and an orphaned tool result no longer kills the conversation"
created = 1788753636
updated = 1788753637
claimant = "Cantaloups-LR3"
priority = 1
root_commit = "12899370c9ec7a5ed7f8e26d3d4fb914ea6c3310"
tags = ["usability-r3"]
+++
The release-train step the auto-merge job waits on (README/release-plz.yml): merge-release-pr holds the open release PR until CHANGELOG.md names the version that PR proposes. The open PR proposes 0.0.13; `make promote-changelog VERSION=0.0.13` stamps the accumulated [Unreleased] section — bl-2d93's ruling that a tool call and its result are one unit for every cut, with the assembler dropping an orphaned result under a recorded note instead of sending a history every provider refuses — as that version. No code moves.