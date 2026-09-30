+++
title = "pre-commit collapses to exec bl-gate (userconf); gate wording in AGENTS/README"
created = 1790735944
updated = 1790735945
claimant = "Junketing-collapse2"
root_commit = "12899370c9ec7a5ed7f8e26d3d4fb914ea6c3310"
+++
ops bl-3166 (phase 2 rollout). The gate body lives once in ~/userconf/bin/bl-gate. .githooks/pre-commit keeps only the mainline refusal (about the ref, not the tree) ahead of `exec bl-gate "$@"`. make check is already the whole gate (bl-2311); nothing to fold.