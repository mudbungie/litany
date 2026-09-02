+++
title = "role config gains effort: the role assignment carries a reasoning-effort level to the adapter on every model call"
created = 1788321089
updated = 1788321090
claimant = "Dial"
priority = 2
root_commit = "12899370c9ec7a5ed7f8e26d3d4fb914ea6c3310"
+++
**Operator ask (2026-09-01).** Seat surfaces want a control for how much effort the model spends (reasoning effort / thinking budget). The one home for that fact is the role assignment (ARCH §4.3): per-workspace scope like provider/model, and since follow-the-tip (bl-403b) control resolves from the governing lineage's current tip at every step boundary — so an edit reaches the very next model call with no new mechanism, which is exactly the seat's switch-mid-conversation semantics.

**Shape.** `roles.<role>.effort: low | medium | high`, optional. Absent = no reasoning requested — the canonical request's `reasoning` stays null, brazen's default behavior, the general path with empty inputs.

**The pinned adapter already speaks it.** brazen 0.0.6 (the exact Cargo.toml pin) lifts the knob: `CanonicalRequest.reasoning: Option<ReasoningEffort>` — "a canonical low|medium|high each protocol maps to its native reasoning shape in encode — a lifted knob, NOT extra" (brazen src/canonical/request.rs). openai/openai_responses project the native reasoning shape, anthropic derives budget_tokens from the shared `ReasoningEffort::budget()` table, google_genai projects too. No pin bump, no brazen release wait.

**Vocabulary.** The config speaks **effort** — the canonical word; "reasoning" / "thinking" are provider spellings that never leave the adapter. Define the term in-line at ARCH §4.3 where introduced (§2.1 rule).

**Changes.**
- `src/config/per_repo_providers.rs`: `RoleAssignment` gains `effort: Option<Effort>`; a litany-owned `Effort` enum (serde lowercase + JsonSchema — brazen's `ReasoningEffort` lacks JsonSchema, so the config type is ours and the conversion is one match at the request-build seam).
- `src/prompt/resolve/mod.rs` `WorkerConfig` + `src/prompt/dispatch/resolved.rs` `Resolved` carry it.
- `src/prompt/dispatch/model_call.rs::build_request` gains the param and sets `reasoning`; both drivers (`exchange.rs`, `advance/hop.rs`) pass `resolved.effort`.
- Docs: ARCH §4.3 (field + term definition), §4.4 invocation note (the request now sets `reasoning` from the role's `effort`). `template/providers.yaml` stays as-is — absent is the shipped default and the field is an operator edit.

**Explicitly not this ball.** `priority:` (fast/priority tokens, provider service tiers) — brazen has no lifted service-tier knob yet; that is a brazen ball first (filed on the brazen board), and the litany field follows the published knob plus pin bump in its own ball.

**Gates:** tests (100% floor, all pass), docs, alignment (ARCH/PRINCIPLES/TAXONOMY).