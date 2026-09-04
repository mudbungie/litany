+++
title = "Stale brazen pin: Cargo.toml pins =0.0.7 while brazen 0.0.8 is published, and yog cannot move until litany does"
created = 1788493189
updated = 1788493216
claimant = "Spellbind-Y"
priority = 2
root_commit = "12899370c9ec7a5ed7f8e26d3d4fb914ea6c3310"
+++
brazen 0.0.8 (published 2026-09-03) carries the change yog's §8.3 flow rule needs: `bz --list-providers --json` gains a `device` column naming the headless sign-in a row serves, and the builtin `openai-chatgpt` row declares one (OpenAI's own device-code wire, `style = "codex"`, upstream brazen bl-6680).

The exact pins make this a chain, not a choice. litany pins `brazen = "=0.0.7"`; yog pins `litany = "=0.0.6"` AND `brazen = "=0.0.7"`, deliberately — the lockfile is yog's own parity check that exactly ONE brazen resolves (yog DESIGN §16.7, "skew death is structural"). So bumping yog's brazen pin alone does not resolve:

    error: failed to select a version for the requirement `brazen = "=0.0.7"`
    candidate versions found which didn't match: 0.0.8
    required by package `litany v0.0.6`

What this ball is: bump `brazen` to `=0.0.8` in litany's workspace manifest, confirm the suite is green (the release renamed `[provider.oauth]`'s `device_url` to `device = { url, style }` — litany authors no oauth row, so nothing here should read it), and release, so yog can bump both pins together.

Blocking downstream: yog bl-7c9f (branch the engine-side `bz --login` spawn on the new device column).