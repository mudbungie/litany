+++
title = "brazen pin → =0.0.19 (presets are catalog, routing table = named rows + credentialed presets; bare bz --login lists the sign-in catalog)"
created = 1792729269
updated = 1792729369
claimant = "Gelded"
priority = 1
root_commit = "12899370c9ec7a5ed7f8e26d3d4fb914ea6c3310"
tags = ["release"]
+++
## Why
yog links brazen and litany and its lockfile must resolve exactly ONE brazen; brazen 0.0.19 (bl-6feb: the routing table / catalog split, bare `bz --login` catalog listing, bearer rows need no api_header, OPENAI_API_KEY/MISTRAL_API_KEY ambient) is publishing now. litany is the other pin (`brazen = "=0.0.18"` in Cargo.toml [workspace.dependencies], the one home; Makefile BRAZEN_PIN and the load-time guard derive from it).

## Behaviour litany may feel
A `bz` run with no `--provider` and no credentialed/named row now exits 78 'no provider resolved' instead of 77 'no credential for provider'. litany's role bindings name providers explicitly; check any fixture/e2e asserting 77 on an unconfigured provider, and any test harness relying on the built-in `ollama` preset being routable unasked (it now needs `[[provider]] name = "ollama"` or `--provider ollama`).

## Done when
Pin = 0.0.19, lockfile resolves it, suite green, landed on main so release-plz refreshes PR #24 (v0.0.14) and publishes.