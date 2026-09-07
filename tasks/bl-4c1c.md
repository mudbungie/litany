+++
title = "the step record names the provider row it ran through: meta.json gains provider beside config_commit and workflow_commit"
created = 1788745724
updated = 1788745724
priority = 3
root_commit = "12899370c9ec7a5ed7f8e26d3d4fb914ea6c3310"
tags = ["usability-r2"]
+++
Upstream ask from yog (VISION §6 litany item 8; design ball yog bl-d13d, DESIGN §3.5 "Money everywhere tokens are said").

yog prices a step's tokens by the pair `(provider row, model)`, because the row a call went through is what a token costs: a subscription row bills a model id at nothing marginal, an API-key row bills the same id at list. The model is already on disk in the step's `request.json`; the row is not — and litany already knows it at the moment the step is recorded, since `prompt/adapter` spawns `bz --json --provider <row>` with the role's `provider:` from the config commit.

Ask: `meta.json` (`src/prompt/step.rs`, the struct that carries `commit`, `config_commit`, `workflow_commit`, `started_at`, `ended_at`) gains `provider: Option<String>` — the exact string handed to `--provider`. Recorded, never computed (ARCH §2.3): it is the fact the adapter was invoked with, not a resolution. `None` only where the adapter was not invoked with one (the same shape `config_commit` takes in a pre-commit step).

Policy-blind: litany learns no rate, sums nothing new, and the budget derivation is untouched. Until this lands yog matches a record naming no row by model alone — priced when exactly one table row prices that model, unpriced when more than one does — so the ask is additive and nothing on either side waits on it to be correct, only to be exact.

Done when a step directory written by a pinned litany carries `provider` in `meta.json`, the field round-trips through the record's own tests, and ARCH §2.3's step-record listing names it.