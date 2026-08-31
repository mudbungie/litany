# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

One bullet per shipped change, imperative mood, with the `[bl-xxxx]` task id as
the trail back to the task that delivered it. Verification-gate closes (the
`tests` / `docs` / `alignment` subtasks every task carries) are process rather
than product and are not listed — they live in git and in the balls store.

This file is hand-maintained and is the only changelog authority: release-plz
never writes it (`changelog_update = false` in `release-plz.toml` — the
rationale lives in that file's header, bl-7558). Every delivery adds its own
bullet under `## [Unreleased]`; at release time `make promote-changelog
VERSION=x.y.z` stamps that section as the new version, before the release PR
merges.

That "every delivery" is **enforced, not merely asked for**:
`tests/changelog_completeness.rs` asserts that every `[bl-xxxx]` id on `main`
since the last `v*` tag appears in this file, with gate closes and
id-less subjects (merges, the release bump) the only exemptions. It runs
under `make check`, so a missing bullet fails the next close rather than
reaching a release.

## [Unreleased]

- **decline a compactor's nomination of `goal.md`, `soul.md` or `name`, the
  same way the dispatch entry is already declined.** A compactor writes its own
  three at its dispatch commit, so nominating one afterwards is a deletion
  inside the `dispatch..tip` range the landing classifies as the compactor's
  product: it landed as a `git rm` against the *dispatching* branch, which
  would then keep stepping with no goal, no soul or no identity line on every
  later model call. Never observed in the wild, and closed on the same
  knowable-from-the-path argument the dispatch entry was. One predicate, not
  two — `is_dispatch_entry` became `not_compaction_eligible(path) ->
  Option<&'static str>`, naming which rule fired, and
  `Error::DispatchEntryNotEligible` became `Error::NotCompactionEligible {
  path, what }`. The system slot's file set also gains one home
  (`dispatch::step_commit::SYSTEM_SLOT_FILES`), read now by the three rules
  that had each spelled the triple out. [bl-541b]

- **give every operator notice on stderr the stable prefix `litany: notice: `.**
  A driver's stderr carries two populations a reader cannot tell apart: the
  Ok-path declines (a compaction landing declined or superseded, a budget stop,
  an accepted-crash launch note, a retarget decline, a settled crashed tool
  window, a budget-refused procedure dispatch, a `setpgid` that did not take)
  and whatever a dying process writes on its way out. A consumer capturing
  `steps/<agent>/driver.log` had to separate them by matching the prose, which
  broke on every rewording. The prefix is the contract — a line carrying it
  means the process continued and its exit code is untouched — and the sentence
  after it stays free prose. One home (`src/prompt/notice.rs`), one emitter
  (`notice!`), so a site cannot misspell or forget it. Exit codes and a verb's
  own confirmation to a present operator are unchanged. [bl-9495]

- **record the compactor-pair ruling in `docs/DESIGN_TOOL_INJECTION.md` §7:
  the host answers `write_summary` / `mark_for_deletion` itself, as engine
  acts.** The bullet had listed three candidates and read as open long after
  the composer adjudicated it (yog `docs/REMOTE.md` §5.4, landed against the
  0.0.2 pin). It now carries the answer, the subject-locality reasoning it
  follows from, and the residual a composing host must know: the built-ins
  read the calling agent's identity from the **process** environment, so a
  linked host still has to re-enter `<driver_target> tool <name>` as a child
  to carry a per-invocation identity. No code changes — the surface the
  ruling uses is the front door that already exists. [bl-43aa]

- **gate the image the way the commit is gated: `make image-scan`, and it runs
  as the last step of `make image`.** `make leak-scan` reads the git index, so
  nothing had ever read what a `podman push` would publish — the build context
  as the engine receives it, the base layers, the package index, or the image
  config. The scan reads three surfaces with the same `scripts/leak-rules.sh`
  table, sourced and never copied: every file whose bytes differ from the
  pinned base at that path, that image's `Env`/`Label`/history, and the
  symlinks in between. The distro floor is **accounted for rather than
  exempted** — apk's own ownership ledger says which package owns each of the
  files `apk add` left, and everything else above the base is this repo's.
  Findings locate and never reprint; unreadable is rejected, not skipped, with
  the expected binary set derived from the Containerfile's `COPY --from=`
  destinations instead of typed. Both directions, because a scan that has
  stopped matching passes everything forever: the self-test layers a fabricated
  secret into a file, another into an `ENV` and an undeclared binary beside
  them, and requires all three findings before the real image is scanned. This
  is the condition the registry ruling came with (yog `docs/DESIGN.md` §10.1):
  `ghcr.io/mudbungie/litany`, pushed only from the release workflow at tag
  time, immutable version and digest tags, never a moving `latest`. [bl-f963]

- **ship as an OCI image: `Containerfile`, `make image`, and the XDG roots it
  mounts.** A fourth install route, for a box that takes images rather than
  binaries. Two stages: the build runs under the toolchain
  `rust-toolchain.toml` pins — checked against the base image inside the build,
  so the `FROM` tag cannot drift from the pin — and the runtime layer carries
  only what the engine execs, which is `git`, `sh`, `bz` and `litany` itself.
  That list is why `FROM scratch` is wrong here whatever the static-musl
  linking story says, and the reasoning sits in the file beside each entry.
  `bz` is installed at the pin read out of `Cargo.toml`'s `brazen = "="` line,
  because a route that shipped the engine without the adapter would not be an
  install route. **The image carries no harness state:** it sets the XDG
  variables and provisions nothing under them, so both roots are mounts, and it
  deliberately does not run `litany prime` — seeding into a layer puts the one
  state litany owns where a mount cannot replace it. `make image` autodetects
  podman or docker and tags from the crate version; it pushes nothing and there
  is no `push` target. [bl-6467]

## [0.0.2](https://github.com/mudbungie/litany/compare/litany-v0.0.1...litany-v0.0.2) - 2026-08-29

- **the seam inverts: the router answers every tool invocation, and the
  driver's local executor is deleted.** `route()` returns a `RoutedCapture`
  rather than an `Option`, so its scope is total — a name the host does not own
  is a refusal the host renders in band, never a hand-back to the three-hop
  binary resolution. The binding therefore picks one execution pipeline for the
  whole process (an installed injection routes every tool; no injection spawns
  every tool), leaving no per-invocation choice for two adjudication stories to
  hide behind. The executor still owns everything around the answer — it lands
  `input.json` and resolves the caller before either backend, then renders the
  envelope, maps `is_error`, applies the bounded projection and writes
  `output.json` — and both backends now produce the same `RoutedCapture`, so
  that is one implementation instead of two that must agree. The exec binding
  keeps its spawn, priced in `DESIGN_TOOL_INJECTION` §3.4 [bl-a00a]
- make `promote-changelog` era-aware at the bl-2f58 rename fence. Its duplicate
  guard matched a bare `## [x.y.z]` heading, which the lernie era already
  supplies for every number up to 0.0.11 — so it would have refused those
  litany versions forever — and its compare link named bare `v<prev>...v<version>`
  tags while litany-era tags are `litany-v<version>` (`release-plz.toml`'s
  `git_tag_name`), so the link would have pointed at two tags that do not
  exist. A heading's compare URL is now the era predicate, defined once and
  used by both the guard and the previous-version read; the workflow header's
  stale `v<version>` tag spellings are corrected with it [bl-4afc]

## [0.0.1](https://github.com/mudbungie/litany/compare/v0.0.11...litany-v0.0.1) - 2026-08-29

- **The engine crate is renamed `lernie` → `litany`, and the `lernie` name
  passes to a sibling component at a version fence.** Under the four-component
  split (yog the server, lernie the seat, litany the engine, thrall the foot),
  this crate — the agent-loop engine — continues its line under the name
  `litany`. **The engine's line under the name `lernie` ends at 0.0.x; a
  `lernie` release numbered 0.1.0 or above is a different component, the
  seat.** That version fence is the only rule that disambiguates the two eras
  of the name on crates.io; read a `lernie` version against it before assuming
  which component you have. The rename carries three durable-state surfaces
  with it, and each is a migration for an existing install:
  `LERNIE_HOME` → `LITANY_HOME`; the XDG harness roots
  `$XDG_CONFIG_HOME/lernie` and `$XDG_DATA_HOME/lernie` → `.../litany`; and the
  in-workspace mark namespace `refs/lernie/*` → `refs/litany/*`. Every other
  `LERNIE_*` variable is renamed likewise — of these, **`LERNIE_CONV_REPO` and
  `LERNIE_CONV_BRANCH` are a published contract to operator-authored tool
  scripts** under `<harness-root>/tools/`, so a script reading them by name
  must be updated to `LITANY_CONV_REPO` / `LITANY_CONV_BRANCH` or it will read
  an unset variable. The reasoning, the full surface census and the migration
  recipe are in `docs/DESIGN_ENGINE_RENAME.md` [bl-2f58]
- Bump the brazen pin to =0.0.6 so litany publishes ahead of its downstream [bl-8ba7]
- Scrub the live operator home path from docs, adopt the leak-scan disclosure gate (rules table, scanner, fixtures, `make leak-scan` in lint, daily `store-scan.yml`), and except GitHub's own noreply committer addresses from the personal-email rule [bl-d0d1]

### Changes
- **bill a cached prompt slice once, not twice, when deriving §6 spend.** The
  derivation summed all four brazen `Usage` counters, which is right only where
  they are disjoint slices — true on Anthropic, false on the OpenAI-shaped and
  Google decoders, whose prompt counter already *contains* the cached one
  (`prompt_tokens` ⊇ `prompt_tokens_details.cached_tokens`, `input_tokens` ⊇
  `input_tokens_details.cached_tokens`, `promptTokenCount` ⊇
  `cachedContentTokenCount`). A step's tokens are now `max(input, cache_read +
  cache_write) + output`: exact where the slice is contained, a floor where the
  counters are disjoint, plain `input + output` where no cache counter is
  reported. The old sum inflated with the cache hit rate — worst exactly where a
  long conversation is cheapest, measured at +25% over one agent tree — so a
  declared `max_total_tokens` ended conversations well before the number it
  reads. Which counters overlap is the adapter's fact, not the harness's; the
  fold collapses back to a plain sum if brazen ever guarantees disjoint slices
  (filed there as brazen bl-d192) [bl-68f5]
- **the armed commit-identity guard judges a `Co-Authored-By` trailer by the
  identities it names**, instead of refusing every trailer outright. The
  blanket rule contradicted the guard's own CI-bot allowance: GitHub's merge
  button stamps `Co-authored-by: github-actions[bot]` onto a squashed release
  PR, so the identity release-plz authors as was allowed in the author slot and
  refused in the trailer of the same commit — `v0.0.11` then failed the guard
  on the one machine that arms it, reddening every close there regardless of
  what was being closed [bl-68f5]
- **keep the conversation's opening prompt through a compaction.** The first
  prompt was written to disk twice at dispatch — as `goal.md` (ARCH §2.8) and,
  through the front door, as the dispatch entry `messages/001-…` (§2.11) — and
  the compactor's own goal quotes `goal.md` verbatim, so the one transcript
  entry a model told to nominate superseded files reads as pure duplication was
  the one the operator reads. It was nominated, marked and squashed away on
  every compacted branch; later user messages have no duplicate and always
  survived. **The goal is not compaction-eligible** (§2.7): `mark_for_deletion`
  now declines the dispatch entry in-band, at the nomination, so nothing is
  staged and the compactor's summary is never premised on a deletion that did
  not happen. The duplication itself stands — it is one input projected into two
  places by one dispatch, neither ever rewritten — because a `goal.md` derived
  from the transcript would change identity the moment compaction shed an early
  entry. [bl-898f]
- **ship the workspace template with no `budgets:` block**, so a new workspace
  is unbounded on tokens, wall *and* depth (operator ruling 2026-08-16). The
  ceilings that shipped — 2,000,000 tokens, 3600 wall seconds, depth 4 — are a
  *whole-tree* allowance a root and its entire descent share, so they bind far
  earlier than the numbers read and ended ordinary agent trees that were
  working correctly, `max_wall_seconds` worst of all. The remedy is off, not
  generous: raising the numbers moves the same cliff. Config deletion only —
  an absent block and an absent key already read as unbounded with no
  compiled-in fallback, so no code path changed and the bounded shape stays
  fully supported and tested. **Existing workspaces keep their ceilings:** a
  workspace freezes its own `workflow.yaml` in its config commit at creation
  and nothing re-reads the template into it, so the exits are the ordinary
  config ones — `lernie config` to author a commit without the block, `lernie
  retarget` to move a running agent onto it [bl-8dea]
- **a minted agent name is now two words in PascalCase** (`PeachHollow`), not
  one lowercase word: a lone common noun in a conversation row or a `lernie
  list` line reads as a word that happens to be there rather than as a name.
  The mint's pool becomes the ordered pairs of *distinct* wordlist entries —
  `n * (n - 1)`, 292,140 names off the same 541 words — which is the index
  space widened rather than a second draw, so the pure single-draw scan, the
  wraparound collision retry and the exact exhaustion bound are untouched and
  no second wordlist exists to drift. The wordlist data is unchanged (its
  count-and-digest approval pin still passes byte-for-byte). The pair walk is
  deliberately not uniform — a collision steps to the next second word, not to
  a fresh random pair — exactly as the one-word walk was not uniform over
  words; uniqueness is the occupied-set check's, not the generator's. A
  supplied name is unaffected: PascalCase already passed every
  `require_available` gate, and a minted name now carries no hyphen at all, so
  it can never be misread as two segments of an id's hyphenated descent
  [bl-79a2]

### Fixed
- Resolve the changelog guard's last-release tag by nearest reachable `v*` tag
  rather than `git describe`, whose committer-date walk answers three releases
  back under date skew and false-fails the guard on ancient deliveries
  [bl-d11e]

## [0.0.10](https://github.com/mudbungie/lernie/compare/v0.0.9...v0.0.10) - 2026-08-15

### Fixed
- Judge the §2.3 tool pairing over the whole alternation when deriving a hop's
  warrant, so delivered mail behind a crash-orphaned tool window no longer
  masks the unpaired decline into an eternally provider-rejected model call
  [bl-15f0]
- Settle a markless unpaired trailing window at the drive boundary, before
  delivery: a crashed executor's window gets an in-band died `tool_result` per
  unanswered id, so an ordinary deposit revives the branch instead of meeting
  the unpaired decline forever; only the buried pre-settlement form still
  declines [bl-4187]

## [0.0.9](https://github.com/mudbungie/lernie/compare/v0.0.8...v0.0.9) - 2026-08-14

### Changes

- a **linked binding can now inject tools of its own**: `cmd::Fx` gains an optional `tool_injection`, one object carrying both halves — the tool definitions to declare and a router consulted ahead of the ARCH §3.3 binary resolution, answering the invocations it owns and declining the rest. Both halves ride one object because either alone is a defect (a tool declared and not permitted is announced and then refused; one permitted and not declared is never called), and the declaration is read off the **executor** — the thing that will answer the call — so prompt assembly, the grant gate and the router cannot disagree. A routed invocation is indistinguishable downstream from a spawned one: the router answers in the stdio contract's own vocabulary (stdout, stderr, exit code), so the result envelope, `is_error`, the bounded projection and the `input.json`/`output.json` record are the same code, and the record stays the executor's to land rather than an obligation exported to the host. What the router owes — its own deadline, a vanished endpoint rendered as a non-zero result rather than a hang, an eye on the cancel flag — is stated at the contract, since it runs on the executor's thread where no SIGTERM cascade reaches it. Contained by construction: tools are individually named (never a multiplexer, the `docs/DESIGN_MCP_BRIDGE.md` §6 ruling now binding on the host), the set changes only by the embedder's act so ARCH §5.5's prompt-cache discipline holds, and the tool control still adjudicates every invocation before anything is routed. No new verb and no new config key. The compactor's own injection was generalized into the same mechanism — `compactor::builtin_tool_schemas(role)` returns the injected list and the separate names half is deleted, one list read two ways instead of two functions held in step by a test. Design record: `docs/DESIGN_TOOL_INJECTION.md` [bl-9001]

## [0.0.8](https://github.com/mudbungie/lernie/compare/v0.0.7...v0.0.8) - 2026-08-13

### Changes

- **the agent-name pool was CC BY 4.0 data in an MIT package, and it minted hostile names.** `src/workspace/agent_name/mint/words.txt` was 7,395 words derived from EFF's Long Wordlist, with its CC BY 4.0 attribution riding into the binary while `Cargo.toml` and `LICENSE` told users MIT-only — and it carried `carnage`, `cruelty`, `evil`, `humiliate`, `traitor`, `wrath` and their kin, two of which were minted as real agent names. It is replaced — not filtered, since a filtered copy is still an adaptation and still owes the notice — by 541 words authored for this repository from scratch: concrete, neutral, everyday English (weather, landscape, plants, food, materials, colours, tools, instruments, buildings, animals, textures, crafts), no third-party corpus behind them, covered by the crate's own licence, so the package metadata is now the whole truth. Sized for human review rather than entropy: the occupied set the pool must out-size is one workspace's living agents, a collision costs a scan step and exhaustion is loud, so the binding constraint is that a person can read the list end to end. The approved set is pinned by count *and* digest, and a new semantic-safety test bans a harm-stem vocabulary by substring and a personal-identity list by exact match — the previous tests asserted character shape, uniqueness, `unknown`, and count, and nothing about meaning [bl-b59c]
- **the compaction clock never fired.** `compactor::checkpoint::origin` founded a branch by the `[<agent-id>]` tail alone, and the executor's own transcript commits end in that same tail (`transcript NNN: <origin> [<agent-id>]`), so `git log -n 1` answered with the newest transcript commit instead of the branch's dispatch commit: `commits_since_checkpoint` read 0 or 1 forever, `every_n_commits` never reached its threshold and `every_t_seconds` measured from the wrong commit, and the compaction span's lower bound was mis-derived where no base had landed. The founding pattern now has one home — `role::founding_pattern`, matching the two dispatch subjects exactly (`^(dispatch: .+|step 001: dispatch) \[<id>\]$`), shared by `role::founding_sha` and the clock — so the retarget landing and the checkpoint cannot drift apart. The suite was green because the checkpoint tests used synthetic subjects carrying no tail; the regression pin now builds a branch with production subjects [bl-89f7]

## [0.0.7](https://github.com/mudbungie/lernie/compare/v0.0.6...v0.0.7) - 2026-08-11

### Changes

- back-fill the eight `[Unreleased]` bullets the 0.0.7 window was missing (bl-c3c5, bl-7935, bl-a4d5, bl-32c9, bl-7173, bl-4ae6, bl-ec74, bl-3361), and **guard the convention so this is the last time**. `CHANGELOG.md` is the only changelog authority — release-plz never writes it, and a docs-only delivery is invisible to generation in principle — so the hand-kept list is load-bearing, and a convention nothing enforces drifts: bl-0b1f back-filled 24 missing bullets and eight more went missing in the very next release window. `tests/changelog_completeness.rs` now asserts that every `[bl-xxxx]` id on `main` since the last `v*` tag appears in `CHANGELOG.md`, exempting what the changelog header already calls *process, not product* — gate closes and the `make promote-changelog` release-prep landing — plus subjects carrying no id at all (merges, release-plz's version bump). It reads the whole file rather than the `[Unreleased]` section alone, so it keeps holding across `make promote-changelog`, and it fires only after a delivery has landed — blocking the next close, never the work that missed its bullet [bl-d92b]
- record the design for **cryptographic agent attestation** in `docs/DESIGN_AGENT_ATTESTATION.md`: per-agent keys, executor-mediated signing, and an inference-log witness. A design record, not an implementation — no lernie code exists or is scheduled [bl-c3c5]
- drop two stale tracking claims that outlived their balls: ARCH §3.3 and `prompt/dispatch/tool_step/multi.rs` both cited bl-a690 as the follow-on that would build a deferred piece, and it was closed unbuilt. §3.3 now states the deferral as a **standing position** and `multi.rs` cites the section rather than asserting a ball. Design reasoning unchanged in both; prose and comment only [bl-7935]
- the `dispatch` tool's own surfaces claimed a subagent was less than an agent, and three of the claims were mechanically false: the `goal` field promised "the terminal compacted result" (terminal compaction is deleted, ARCH §2.7 — the dispatcher receives the child's own terminal response with its epitaph) and was silent on the child being addressable while it runs, which reads as a one-shot fork; the `role` field said "v0.4 Phase 2 supports worker" against an open role set (§4.3 enumerates no names), as did `skills/dispatch/SKILL.md`; and "per-conversation spend limits" advertised a per-agent allowance where the mechanism is one frozen whole-tree ceiling with no per-dispatch inheritance. Fixed at each doc comment's source (`config::workflow`, `prompt::dispatch::resolved`, `prompt::budget`) and regenerated, with `template/workflow.yaml` stating the tree-wide rule outright. The operator ruling stands: **an agent is an agent** — "subagent" is a relational word, and what the taxonomy refuses is the *category*, not the term [bl-a4d5]
- `docs/DESIGN_MCP_BRIDGE.md` §9 cited bl-8925 as a filed follow-up; the ball was closed unworked on 2026-08-06. §9 now records it as filed-then-closed-unworked and states plainly that no bridge was built and no MCP server has been round-tripped. The ruling, the refusals and the §7 spec are unchanged [bl-32c9]
- ARCH §3.3 and `install/tests/toolspec.rs` used bare "per-call" for the `cd` working directory — the tool-interaction sense §2.1 bans, in the document that states the rule. Both now read "per-tool-call" [bl-7173]
- ARCH §6 never spelled out the `max_depth` boundary, so how far a child's own children may go was defined by an implementation guess in `budget/mod.rs` that no test meant to guard. §6 now carries it: depth counts dispatches from the root agent (root = 0), `max_depth` is the deepest allowed depth, exhaustion is strict (`depth > max_depth`), `max_depth: 0` means root-only, and an agent at `max_depth + 1` makes no model call at all — with a companion paragraph on why it matters (an agent is an agent, so the ceiling is the tree's one sanctioned circumscription on a child dispatching children, and it bounds depth only, never breadth). The dispatch gate names the same predicate applied to the prospective child. `prompt/tests/budget_depth_boundary.rs` pins the off-by-one at the real child hop; flipping the check to `>=` fails it [bl-4ae6]
- `multi_tool`: **let the envelope assert parallel execution**. The agent, not the harness, knows whether its inner invocations collide, so an envelope may now say `execution: "parallel"` and the harness classifies nothing and verifies nothing. Concurrency lives in the executor: `ToolExecutor` gains `execute_all` with a serial default, which `SpawnTool` overrides by splitting each invocation into prepare / spawn-and-capture / finish and overlapping only the middle phase — so nothing self-dependent crosses a thread boundary and `Clock`, `GitRunner` and `PathLookup` keep their lack of a `Sync` bound. The fan gates every entry before any runs, then hands the survivors over together; results render in list order, `on_failure` is not consulted under parallel, and an inner `cd` and same-path writers stay legal and unpoliced [bl-ec74]
- sweep the tool-call sense of bare "per-call" tree-wide to "per-tool-call", and the model-call sense to "per-model-call" — ~15 further sites the bl-7173 pass did not reach, across ARCH, `USER_STORIES.md`, the README, `prompt/tool/**`, `dispatch/tool_step*`, `adapter.rs`, `TAXONOMY.md` and four test files, leaving the four sanctioned sites untouched. Two corrections beyond the rename: `SpawnTool`'s doc claimed it was "constructed per-call by the loop … scoped to one invocation" when it is constructed **once at the executor's entry point** and scoped to one step loop, and `DESIGN_AGENT_ATTESTATION.md`'s "first call only" rule was the same banned sense, now "first model call only" [bl-3361]
- add `lernie retarget <workspace> <agent> [--config <name>]` — the one exit from ARCH §2.2's config freeze, which until now had none: an agent forked off a config commit was welded to it for life, so an operator who fixed an expired model id watched the very next step dispatch the old one and the only alternative was abandoning the conversation's whole history. The verb writes a **ref mark**, `refs/lernie/retarget/<agent-id>`, at the target config commit and moves no branch; the agent's **own executor** consumes it at its next `advance` step boundary, so §2.3's single-writer invariant is untouched. The landing is a **re-fork**, not a merge and not a graft — the compaction landing's own two moves: a newly minted dispatch commit parented on the target, with everything config-shaped re-derived there (the §3.3 descriptor cut, the control-file removal, the pinned soul) and the old subject kept verbatim, then the agent's own post-dispatch commits replayed on top. `governing_config` — an unchanged, pure ancestry query — answers the target afterwards, with no new stored fact anywhere. The replay is now shared rather than copied: `prompt::rebase_forward` is the one rebase-forward move, and the compaction and retarget landings differ only in the base they mint and the ref a decline marks. Every refusal precedes the mark (unknown workspace/agent/lineage, a grant the target config does not describe), a target already governing the agent is a clean no-op, and the mark is consumed in every outcome [bl-22a5]
- seed a new agent's working directory at creation: `lernie prompt --cwd <path>` and `lernie dispatch --cwd <path>` write the ARCH §3.3 working-directory mark — the same `refs/lernie/cwd/<agent-id>` the `cd` built-in writes and the executor reads at every tool spawn — once the agent's id has settled and before its first step, so a caller can put an agent to work in a checkout it owns. A second writer for one fact, not a second channel: the `cd` tool only exists once the agent is already running, and a caller cannot win the race to write the ref itself (the fork, the goal deposit and the driver launch are one move, so the id is learned only after step 1 may be underway). The directory runs `cd`'s own validation — exists, is a directory, survives the mark's trimmed-UTF-8 round trip — at the binding, before any branch, ref, worktree or inbox exists, and a git that cannot write the mark fails the creation rather than starting the agent somewhere else. Omission is unchanged behaviour (the worktree), and **nothing is inherited**: a child's mark is unset unless its own dispatch names one. The model-facing `dispatch` tool schema is deliberately unchanged [bl-d0b4]
- answer the three cold-start blind spots a first-run walkthrough hits: `lernie prime` now reports what it founded on stderr (both harness roots and what lives in each, plus this run's seed-if-absent split — `0 files seeded, N already present` is the re-run answer, not a second code path), stdout staying product-less per ARCH §3.4; a failed model call names the **provider row** it was routed under — lernie's fact, invisible to brazen, which can only say `<id>` — and a missing credential (brazen's `auth` kind) states the fix with that row substituted in (`bz --login --provider <row>`, its API-key env var, `bz --list-providers`); and the README's built-in-tool walkthrough shows what `lernie tool <name>` actually prints — the raw stdout/stderr/exit-code triple, not the ARCH §3.3 result envelope the harness composes from it — with `cd`'s "try it directly" replaced by the truth that it declines outside a step for the missing `LERNIE_CONV_REPO` [bl-7e9e]
- deflake the shared coverage gate: the driver-log poll in the ARCH §2.11 launcher tests reached its retry sleep only when the fire-and-forget child lost the race to the first read, so on a fast or lightly loaded box that line never executed and `make check` failed the 100% floor on a tree that had passed minutes earlier; the retry budget is now injected the way the sibling lock poll already does it, and a poll for a count the log can never reach exercises the retry and give-up arms on every box [bl-2625]
- the agent-name mint moves into lernie (yog bl-aca4 ruling): every creation path settles a name at its pre-flight — supplied → validated against the living agents, absent → a one-word name minted from the embedded EFF-derived wordlist against the same scan (RNG-start wraparound draw, bounded retry, loud exhaustion) — so no fork ends nameless, `lernie prompt`, `lernie dispatch` and the harness-initiated procedure dispatches included; the `dispatch` tool schema and skill teach `name` as an exposed parameter (what a name buys, that omission mints, that a supplied collision is refused; `required` stays `[role, goal]`); the crate exports the mint seam (`lernie::mint` — `mint`, the `Rng` trait, `SplitMix64`, `MintError`) so yog draws the same function through the crate for its preview and fire, the wordlist staying behind the function [bl-404d]
- settle the tool window a `lernie stop` fells, so a stopped agent stays restartable: the exiting executor commits one in-band `is_error` `tool_result` per unanswered `tool_use` id (the invocation was cut short) before it deposits its `stopped` epitaph, leaving a paired tail that warrants a model call — previously a stop taken inside a tool call left an unpaired tail, the next `lernie advance` declined with `UnpairedToolUse`, and every subsequent deposit was stranded with fork-from-history the only way out; the assistant's own reasoning is kept (the tail is settled, never deleted) and the model reads in band that it was interrupted [bl-b98d]
- capture a detached driver's stderr instead of discarding it: the ARCH §2.11 launcher now binds the child's stderr to `<workspace>/steps/<agent-id>/driver.log` (append-create, inherited across the ARCH §6 exec baton) rather than `/dev/null`, so the operator-facing lines a `setsid` driver has no terminal for — a compaction landing declined or superseded, a launch that failed into the accepted crash class, a budget stop — are on disk instead of nowhere; stdin and stdout stay null (a driver reads nothing and writes no product to stdout), the path is derived from arguments the launch already carries rather than configured, and a sink that cannot be opened declines the launch instead of silently falling back to null [bl-55f9]
- carry the ARCH §2.1 cross-document reference rule into the strings the binary prints: every section citation a user can see is now written `ARCH §N` rather than a bare `§N` — the adapter-missing and version-skew refusals, the config-commit control read, the reserved-model-id decline, the half-stream and unpaired-tool-use declines, the budget refusals and the budget stop, the workflow-action decline, the summary conflict-marker refusal, the governing-config declines, the launch and compaction-landing notices, the undescribed-tool refusal, the credential decline, the harness-root founding report, and the failed-branch advisory; `docs/USER_STORIES.md` US-24, US-06 and US-17 re-quote the corrected text, and the two half-qualified fork-point reasons (`ARCH §2.3, §7.2`) now qualify both halves. Rust doc comments are untouched — a bare `§N` there is still this repo's own ARCH, and no user reads them [bl-da57]
- apply the ARCH §2.1 cross-document reference rule to the two documents that number their own sections while citing ARCHITECTURE bare: every ARCHITECTURE reference in `docs/TAXONOMY.md` and `docs/USER_STORIES.md` now names its document (`ARCH §N`), each states the convention at its head, and a quotation keeps the quoted source's numbering (quoted spec prose and quoted program output are reproduced verbatim) — TAXONOMY's own §4 (*Tools, function calling, MCP*) no longer reads as the ARCH §4 it also cites; the four documents that number their own sections are now the four that comply, and the one remaining gap — the binary's own error strings, half of which cite `§N` bare — is filed as bl-da57 [bl-8766]
- close three doc-clarity hazards: cross-document section references now name their document (the rule is stated in ARCH §2.1 and applied across `docs/DESIGN_MCP_BRIDGE.md`, whose own §6 read identically to ARCH §6), the brazen citations name `specs/architecture.md` by path and quote the four sections lernie binds to, and the §2.1 bare-"call" ban gains an explicit programming-sense carve-out (call site, callback, callee, function call, system call) so the ban reads as what it always meant — no unqualified "call" for a model, tool, or API interaction [bl-1966]

## [0.0.6](https://github.com/mudbungie/lernie/compare/v0.0.5...v0.0.6) - 2026-08-02

### Changes

- address a terminating agent's result message by its epitaph: a **reply** (final response) answers whoever last prompted it — derived from the branch's own transcript, so an operator's question in a child's conversation is answered there and deposits nothing — while an **obituary** (stopped, budget-exhausted, died) still reports to the dispatcher; the §2.11 exit protocol now revives the recipient it deposited into, and the §2.6 work-product transfer and §6 delivered-child-result bindings apply only in the dispatcher's inbox [bl-a96a]
- teach the wait and the do-it-yourself default in the prompt surfaces: the worker soul and the `dispatch` skill now state that ending a step is how you wait (a deposit revives a quiescent agent, ARCH §2.11) and that sleeping or re-checking is a paid model call that learns nothing, and that the goal is yours to execute — dispatch buys separation or parallelism, and reporting that you dispatched is not answering a goal; the `message` and `bash` skills carry the same wait rule at their own point of temptation, and the `dispatch` skill is re-voiced out of the retired "subagent"/"conversation" vocabulary (its `handle` field is named as the child's address, not something to poll) [bl-93e6]

## [0.0.5](https://github.com/mudbungie/lernie/compare/v0.0.4...v0.0.5) - 2026-08-02

### Changes

- prune the fork point's dialog (`messages/**`, `summary/**`, `skills/**`) from every child's dispatch commit, so a child never opens on its dispatcher's conversation and cannot re-execute the parent's last user instruction (the yog bl-d023 spawn-a-subagent runaway); the compactor keeps the dialog it exists to compact, and fork-back-in roots keep the conversation they resume [bl-5a36]

## [0.0.4](https://github.com/mudbungie/lernie/compare/v0.0.3...v0.0.4) - 2026-08-02

### Changes

- apply_patch declines a symlink destination on Add, Update, and Move to — `symlink_metadata` on the final path, so a dangling link can no longer route bytes through to its target (Delete stays exempt: it removes the link itself) [bl-2502]
- match codex's blank-line grammar in apply_patch exactly: a bare blank line in an Add body or between sections declines loudly (write a lone `+` for blank content), blanks after `*** End of File` are ignored, and the Update-body empty-context reading is documented as faithful parity [bl-fdbb]
- exclude zero-run tasks from agent-eval's pass@1/pass@5 means instead of leaking `NaN%` into run and compare reports — unmeasured, not zero [bl-dad5]
- refuse a pinned document whose destination path crosses or lands on a symlink: `--pin` validation promised no traversal, and `write_into` now enforces it at write time too [bl-91f8]
- read the hold mark unconditionally in the tool-execution loop, so a hold whose control was since removed from config resumes cleanly instead of re-running committed blocks and double-depositing a `tool_result` [bl-11af]
- commit the provider's token usage with the model output it belongs to: a `messages/NNN-<model>.json` entry is now an API-shaped `{"content":[…],"usage":{…}}` object (the bare block array stays lawful and unmigrated), so a transcript reader states real token counts from the committed bytes alone [bl-718e]
- the hand-maintained changelog becomes the single authority: release-plz no longer writes CHANGELOG.md (`changelog_update = false`), the bl-1923 subject-protection preprocessor is deleted (release-plz preprocesses twice, so it doubled — 'fleet: fleet: drop …'), and `make promote-changelog VERSION=x.y.z` stamps [Unreleased] as the release section [bl-7558]
- record every result deposit at a durable `refs/lernie/returned/<child>` mark so `lernie scan` never fabricates a `died` epitaph for a compactor (or any child) whose return was consumed [bl-2c06]
- qualify the bare-'call' usages the 8/01-02 doc edits introduced, define tool window / grant gate / checkpoint origin, fix ARCH's rm cross-ref to §5.4, drop the cache-pin overload [bl-81ce]
- retire the compaction-merge story from PRINCIPLES, USER_STORIES, README, and the ARCH shipped-state notes: rebase-forward is the landing, nothing merges [bl-10a2]
- back-fill the 24 missing [Unreleased] bullets for the v0.0.3..main deliveries [bl-0b1f]
- partial compaction with rebase-forward: zero-downtime compression of a commit span [bl-bc9c]
- connection points in the tool-call path: a gate seam, and controls shipped as knobs [bl-de6d]
- design the MCP client bridge as an external tool: one adapter binary opens the integration ecosystem (docs/DESIGN_MCP_BRIDGE.md) [bl-3c76]
- retire the global models table: `install/models.yaml` ships mechanism only (the optional `adapter:` override — no model ids, capabilities, or context windows in git), a role's `providers.yaml` assignment is the whole model binding, and the roles-against-models cross-check is deleted; a leftover `models:` block in an operator's file parses as inert [bl-35e2]
- a multi-tool tool: structured tool calls as arguments, with execution metadata [bl-8ee7]
- an apply_patch-class edit tool: atomic multi-file patch with fuzzy context matching [bl-ae6b]
- caller-supplied pinned documents reach prompt and dispatch [bl-fb5c]
- agent-eval reports quality, wall time, attempts, tools, and usage [bl-36fa]
- assembler read path trusts composed bytes: refuse a composed entry carrying literal conflict-marker lines [bl-c867]
- bound tool output committed to the transcript: head+tail cap with an honest truncation marker [bl-d5fa]
- protect leading 'word:' subjects from release-plz's conventional-commit parser, which rendered 'e2e::advance_cli …' as ':advance_cli …' in the 0.0.2 changelog [bl-1923]
- give the compactor a read tool and compose the summaries and work products its manifest entry names [bl-2c63]
- the agent's name reaches the model through the assembled context, not as prose prepended to the first user message [bl-d55f]
- correct ARCH §5.1's worktree invariant: the manifest IS the inclusion filter, and a worktree file no role names composes into nothing [bl-b415]
- every tool result now carries a result envelope — the exit code stated on its first line, stdout, then stderr under a `--- stderr ---` marker whenever the tool wrote any, on success as well as failure; a model can tell exit 1 from 127 from 143, and a warning from a command that exited 0 is no longer dropped [bl-ffc5]
- fork-from-history and config-branch selection reach the CLI: --from <ref> on prompt/dispatch, --config <name> on prompt [bl-a693]
- guard the seeded `models.yaml`/`providers.yaml` provider names against brazen's actual resolved table in CI, so a shipped row brazen can't serve fails a test instead of an operator's first dispatch [bl-9391]
- agent naming becomes a first-class fact: --name at prompt/dispatch, stored under the agent; message resolves id-or-unique-name [bl-c8ed]
- agents get a cd tool: a builtin that changes the agent's own working directory for all subsequent tool calls [bl-a501]
- lernie delete <workspace> <agent> [--children]: agents gain a lawful removal verb [bl-0d9e]
- clarify the bash tool spec for models — local, non-interactive, worktree-rooted, stateless between tool calls — aligned with codex's shell tool [bl-298c]
- extract fleet/ to ~/ops/fleet: the demo is a consumer artifact and does not belong in the harness repo; README keeps a pointer [bl-b892]
- fleet: drop the bl-a900 grant-union workaround, update README/comments for the five landed fixes, re-run the live e2e green [bl-e5aa]
- a child role's tool grant is no longer silently capped by its dispatcher's: the fork prune reads the child's own grant, so a granted tool with no descriptor in the parent's tree still reaches the child's request [bl-a900]
- model-tool dispatch no longer forks the child between the parent's tool_use commit and its tool_result commit, so a child cannot inherit a dangling tool_use [bl-4231]
- declared-is-not-callable now holds for every role: tool_step::run_tool_calls refuses any tool outside the caller's grant, not just the compactor's [bl-5a1f]
- validate pooled skill frontmatter at prime/new/config snapshot time, so a malformed SKILL.md cannot poison existing workspaces [bl-e3f5]
- work-product transfer no longer deletes the parent's descriptions/**: transfer.rs CONTEXT_EXCLUDES carries 'descriptions' [bl-475a]
- fleet: role-separated agent fleet (coordinator/shepherd/sensor/builder/steward) demo on lernie — souls, role grants, mock slack tool triples, live e2e test [bl-7780]

## [0.0.3](https://github.com/mudbungie/lernie/compare/v0.0.2...v0.0.3) - 2026-07-30

### Changes

- lernie's brazen pin is =0.0.4 while crates.io has 0.0.5: releasing 0.0.3 as-is locks the skew into yog's dependency graph [bl-e4ef]
- agent worktrees carry descriptors for tools the role cannot call: descriptions/** is snapshotted and inherited unfiltered by the grant [bl-18a9]
- runaway recursive compaction dispatch: compactor branches re-trip every_n_commits, max_depth is unenforced at dispatch, and a conflicted compaction merge commits its markers [bl-a9eb]
- shipped template grants the worker role only [bash, read_file, load_skill] — no root agent can message a sibling or dispatch a child out of the box [bl-38c2]

## [0.0.2](https://github.com/mudbungie/lernie/compare/v0.0.1...v0.0.2) - 2026-07-27

### Changes

- Resolve the release-binaries tag from the pinned push sha rather than a
  main-racing `git describe` [bl-8f7c] *(bullet restored 2026-08-15: the
  delivery predates the changelog guard's window and had none)*
- Lost wakeup in the 2.11 deposit-probe-launch protocol: a deposit racing a driver's last inbox read is stranded forever [bl-9c8f]
- record the 2026-07-27 live walk — upgrade live-verified items from unit-only/unchecked, note the two non-findings [bl-be70]
- lernie dispatch skips the shared id/workspace guard: raw git argv on a missing workspace, config-derivation voice on a missing agent, raw sha on an unknown role [bl-c89b]
- lernie config editor-failure decline names neither the $EDITOR value tried nor the fix [bl-79ea]
- lernie config --from <nonexistent> dumps the raw git argv and the internal .config-author path instead of naming the missing lineage [bl-55e0]
- The built-in tool set is undiscoverable from the CLI: 'lernie tool --help' shows a bare <NAME> and the unknown-tool decline lists nothing, while load_skill's sibling decline names its whole pool [bl-4bd1]
- lernie new pointed at an existing FILE says 'I/O error: Not a directory (os error 20)' — the destination guard covers non-empty dirs but not non-dirs [bl-8efa]
- README does not document the opt-in commit-identity guard shipped in tests/commit_hygiene.rs [bl-303d]
- commit-identity guard test + changelog normalization [bl-3ac2]
- lernie scan aborts the entire pass with a raw git error when any agents/* branch's derived parent has no ref (filed upstream as 'exit 1 for a root with pending mail' — that repro is exit 0) [bl-025b]
- e2e::advance_cli baton test outruns its 120s evidence-poll bound when the box runs several full suites at once [bl-2bf0]
- 0.0.1 ships two binary distribution channels (crates.io, GitHub release tarball) that the README documents no path for: no cargo install line, no bz step, and the tarball carries no docs at all [bl-33ef]
- A missing bz surfaces as 'adapter subprocess: No such file or directory (os error 2)' — the one error every binary-install user hits names neither bz nor the fix [bl-63c1]
- lernie advance on a nonexistent agent exits 0 silently AND mkdirs an orphan inbox — the id-existence guard is missing at exactly the verb README says has it [bl-bbba]
- README release-chain sentence describes the pre-bl-a124 containment: 'bl close reaches GitHub, which runs CI, which runs release-plz' is now inverted [bl-19d5]
- parent_revival and sweep_deposits_died flake under load (post bl-6987 sweep) [bl-7318]
- print the linked brazen pin in lernie --version (e.g. "lernie 0.0.1 (brazen 0.0.4)") [bl-c1b9]

## [0.0.1](https://github.com/mudbungie/lernie/compare/v0.0.0...v0.0.1) - 2026-07-26

### Changes

- Derive experiments/baseline/workflow.yaml from template/workflow.yaml instead of copying it [bl-aa9a]
- Scope stop-cascade pgid discovery so it cannot signal the wrong process group [bl-5f0c]
- Add integration coverage for stop --stop-children [bl-3761]
- Bump the brazen pin to =0.0.4 [bl-8c92]
- Package for crates.io: metadata, repo-development excludes, eval-island resolution [bl-8801]
- Resolve a repo-local pinned bz for tests so parallel agents' installs cannot race [bl-8366]
- Implement the spawn_root_agent action the template workflow declares [bl-0e79]
- Fix doc and terminology drift: session POSIX carve-out, README drift, test-double renames [bl-81a0]
- Subtract the inert drop_oldest_steps overflow policy [bl-7846]
- Exit clean on lernie stop during a model call, per the §2.9 contract [bl-5156]
- Deflake spawn_retries_past_transient_etxtbsy, which raced two wall clocks under load [bl-7a3f]
- Install bz in ci.yml so the real-bz e2e tests run on GitHub CI [bl-636b]
- Capture adapter stderr so startup failures no longer masquerade as killed-mid-stream [bl-cd6b]
- Run tests/install.rs in the close gate, outside tarpaulin [bl-f01f]
- Rewrite the README compaction and dispatch narrative off deleted code [bl-a227]
- Keep the compactor's private transcript out of the parent on compaction merge [bl-4fdf]
- Report the real size on read_file oversize, and stop lernie new silently dropping descriptions/ on an unseeded data root [bl-71b8]
- Tear down the transient checkout when a lernie config authoring pass is declined or fails [bl-54aa]
- Preserve a config/* ref through replay so a replayed workspace can be driven [bl-aabb]
- Author docs/USER_STORIES.md, the promise suite 0.0.1 is evaluated against [bl-0135]
- Wire summarize at assembly to trigger the §6 compaction checkpoint [bl-a1a1]
- Switch crates.io publishing to trusted publishing (OIDC) and drop CARGO_REGISTRY_TOKEN [bl-6cfb]
- Strengthen command_surface_parity.rs to argument- and product-level bijection [bl-4762]
- Validate the agent id at the inbox boundary so lernie message errors on a nonexistent agent [bl-bdc7]
- Compose a standalone skill's frontmatter into description-always [bl-ae66]
- Run bash and tool subprocesses in the agent's worktree, not the operator's cwd [bl-2503]
- Launch the parent's driver on child terminal deposit, so revival needs no manual scan [bl-4a6c]
- Fix the coverage-measurement flake: an intermittent one-line miss from llvm region attribution [bl-1c2e]
- Inject the adapter target through Fx, and publish to crates.io [bl-612a]
- Bump the brazen pin to =0.0.3 [bl-143e]
- Correct the release-plz.yml header comment: four jobs, including prune-release-branches [bl-6b52]
- Prune superseded release-plz-* branches in release-plz.yml [bl-2e35]
- Add build-gated release-plz release automation [bl-2e8d]
- Add the live-wire smoke test, the first real model call (make smoke) [bl-06d5]
- Split child_result.rs back under the 300-line code cap [bl-19cb]
- Give the Terminal(Epitaph) and AdvanceHandoff::Done payloads a runtime reader [bl-4f6d]
- Wire manifest.yaml into §5.2 context assembly [bl-e0cb]
- Seed lernie new control files from the config-root template/ override, with the embedded fallback [bl-e795]
- Unify the §3.3 tool-resolver third hop with the injected driver target [bl-eb98]
- Wire the config machinery — manifest, version, loaders, schema generation — into the verbs [bl-9e2d]
- Update the README GUI pointer: the sibling repo is now yog, not lernie-ui-egui [bl-60f7]
- Expose the command surface as a library for exact-pin consumers, and parametrize the driver/successor exec target [bl-231c]
- Add lernie prime: idempotently found the installation substrate (config root plus data root), honoring LERNIE_HOME [bl-6d83]
- Open the dispatch role set so lernie dispatch <role> accepts any config-defined role [bl-f72b]
- Define the nature of a role: what it is, what it binds, where it lives [bl-3a85]
- Land the workspace substrate: config branches, agents/* refs, no main (§2.2–§2.3 physical) [bl-a51c]
- Correct the invalid worker-default model id that no mocked call could catch [bl-3157]
- Clear repo-wide rustfmt drift and pin the toolchain [bl-f830]
- Land the evaluation layer: task suite, agent-eval, bundle/replay (§9, v0.9–v0.10) [bl-8094]
- Decouple the GUI: extract lernie-ui-egui to its own repo and strip the crate plus tethers from lernie [bl-bdd2]
- Derive BRAZEN_PIN from Cargo.toml, the pin's single home [bl-51ef]
- Set the root version to 0.0.1 for the first crates.io release [bl-58a8]
- Tidy publish hygiene: publish=false on lernie-ui-egui, repository field on the root [bl-a908]
- Complete workflow actions v0.7 (following bl-6a3b): the remaining executors and the verifier gate end-to-end [bl-bd8a]
- Wire the compaction bindings live: dispatch(compactor) on checkpoint, compaction_merge on return, role-aware advance [bl-2d5a]
- Add agent-eval --config/--suite/--runs (v0.10) [bl-25fc]
- Land real compaction: model-driven compactor, checkpoint triggers, the compaction merge (§2.6–§2.7) [bl-9dbd]
- Land workflow actions: event-to-action bindings act at runtime (§6, v0.7) [bl-6a3b]
- Land the child step loop: dispatched children run to terminal and deposit (§2.5 live) [bl-c33b]
- Add skill body-on-demand: agent-elected skill load into the worktree (§3.3) [bl-4af9]
- Document the exec baton as structural state-safety (§6, §3.1, PRINCIPLES) [bl-0855]
- Add the config-commit authoring verb: harness-assisted config edits beyond lernie new [bl-2774]
- Add the §7.1 workspace view and the §3.5 agent-state classification [bl-46b4]
- Add lernie advance, the driver verb: exec baton, lease handoff, launch detachment [bl-4684]
- Wire SIGTERM to the run_tool_calls stop flag so stop during tool execution exits on a clean stopped deposit [bl-f2a8]
- Segment scan into driver, reviver, and janitor so crash-rate events do not run at step frequency [bl-5846]
- Add the agent-level stop cascade: --stop-children walks the id namespace, kernel pgid scoped to one executor [bl-535d]
- Discover the executor pid via the lock fd rather than response.json in lernie stop [bl-aafc]
- Re-voice the §3 process model and note NFS as an unsupported configuration [bl-8122]
- Deposit the stopped epitaph from the SIGTERM handler [bl-9f53]
- Add the startup scan: silent-death sweep deposits and inbox flush [bl-d148]
- Delete merge/: the result-message return replaces rebase-then-merge-back [bl-4ce8]
- Rename the assistant origin token so the model id authors the entry [bl-79aa]
- Add the delivery drain: step-boundary inbox drain and delivery commits [bl-1129]
- Re-point the context assembler at the transcript, and delete the accumulator and output.json read path [bl-26cb]
- Add the inbox substrate: executor flock, deposit, the lernie message verb, the message tool [bl-cb44]
- Add the transcript writer: stream assistant output to staged transcript entries [bl-4798]
- Design the §2.11 inbox substrate: deposit, delivery, executor lock [bl-4298]
- Design the §2.3 transcript writer: context has one home [bl-a847]
- Delete await_tool: await and check have no referent once step 5 is total [bl-2ca1]
- Dissolve await: every terminal event deposits a result message [bl-65d8]
- Wire the descriptions-always producer to populate descriptions/{tools,skills}/ at conversation-repo creation [bl-3092]
- Design the workspace substrate: one repo, config branches, agents as worktrees, merge-back eliminated [bl-b09c]
- Give context one home: committed transcript, no in-RAM history, append-only assembly [bl-c904]
- Split the harness root along XDG lines into config and data dirs, with LERNIE_HOME collapsing both [bl-c6e7]
- Adopt terminology ladder v2 — workspace, config, agent — with exchange demoted to a span [bl-08df]
- Collapse §6 budgets to one live whole-tree check and delete clamped inheritance [bl-f48c]
- Settle message delivery versus blocking await for reminder-shaped children (§2.5, §2.11) [bl-2ff2]
- Compose role tools into the model call by loading descriptions/tools/*.json into CanonicalRequest.tools [bl-9e96]
- Record messaging as a deferred lateral prompt-injection surface (§11) [bl-962d]
- Add per-conversation budgets: workflow.yaml max_total_tokens/max_wall_seconds/max_depth, spend derived from disk, checked at the step boundary (§6, v0.7) [bl-9e9b]
- Amend §2.11 for writer and driver totality [bl-3eea]
- Scrub the inherited git env from tests/prompt_retry.rs, and correct stale providers.yaml references in per_repo_providers.rs [bl-4a9b]
- Swap the data plane to brazen bz: exec per attempt, harness-owned retry, config fold, delete lernie-provider-anthropic (§4.4, §2.10, §3.5, §4.1–§4.3) [bl-56ee]
- Design front-door messaging (§2.11): the queue model [bl-ed40]
- Add the additive brazen read-side transition: src/provider/segment.rs classifies a closed response.json per §4.4, with the dual-vocabulary seam isolated in one function [bl-507a]
- Author ARCHITECTURE §3.6, "Sandboxed tools (v1.1)" [bl-0bae]
- Resolve six attempt-segment spec holes in docs/ARCHITECTURE.md [bl-5811]
- Author ARCHITECTURE draft v0.4, the harness-repo fold: brazen is the provider layer, and §4 is rewritten around the bz pipe contract [bl-f739]
- Add the make ui REPO=<path> target [bl-4d80]
- Detect kill-mid-stream subagents via a /proc fd scan [bl-c9ec]
- Add the UI new-prompt and stop buttons via cli_outbound (v0.5 P11) [bl-163d]
- Add lernie stop: cascading SIGTERM to the executor's process group, pid discovered by /proc fd scan (§2.9, v0.5 P10) [bl-a144]
- Resolve dispatch handles from the await built-in (v0.4 P3) [bl-829f]
- Amend ARCHITECTURE and PRINCIPLES to excise user-action resume in favor of lernie advance [bl-abf3]
- Spawn subagents from the dispatch built-in via the §3.4 CLI (v0.4 P2) [bl-175a]
- Generalize the lernie dispatch CLI for arbitrary roles (v0.4 P1) [bl-88f4]
- Add derived branch-state indicators (in_flight, stopped) to the UI, and amend §2.9 to drop the cancel marker [bl-de6b]
- Pin tarpaulin 0.35.2 to stop a silent coverage-denominator shrink [bl-ae88]
- Bump actions/checkout to v6 and actions/cache to v5 [bl-3db1]
- Stream response.json as JSONL with writer-closes-fd completion (v0.3.1 P3) [bl-584d]
- Configure the git identity on the CI runner [bl-8f56]
- Add the llvm-tools-preview component for tarpaulin [bl-45e7]
- Relocate the harness step record and simplify the snapshot (v0.3.1 P2) [bl-10dd]
- Amend ARCHITECTURE to move step records out of the worktree (v0.3.1 P1) [bl-4117]
- Add a GitHub Actions CI workflow running make ci [bl-e99b]
- Split the user quickstart from the contributor setup [bl-b8ec]
- Add the bash built-in tool (v0.3) [bl-ecf1]
- Scaffold the harness root from make install (phase 7) [bl-66da]
- Add the read_file built-in tool (v0.3) [bl-a96a]
- Add the multi-step exchange loop (v0.3) [bl-9221]
- Update the UI fs_watcher prefixes and git_tree renderer to the v0.3 shape [bl-3465]
- Add the tool executor: SpawnTool plus a per-call disk record (v0.3) [bl-7c2a]
- Regenerate the schemas and sweep the last strings (phase 6) [bl-d135]
- Write compactor output to summary/<NNN>.md under merge=ours discipline (phase 5) [bl-d944]
- Add the tool_use and tool_result wire types plus the tools request field (v0.3) [bl-8be5]
- Add the soul and role machinery, and retire agents.yaml (phase 4) [bl-70b9]
- Pin the v0.3 tool contract in ARCH §3.2, §3.3, §4.3, §11, §12 [bl-fdb9]
- Migrate the prompt path to v0.3 (phase 3) [bl-7bca]
- Add the adapter streaming wire (§4.4) [bl-de80]
- Add the v0.3 conversation-repo template and scaffold (phase 2) [bl-ecda]
- Add the harness root and split global from per-repo providers (phase 1) [bl-d7b1]
- Redesign the conversation-repo layout and retire invocation as a structural term [bl-32ab]
- Add Regenerability to ARCHITECTURE and PRINCIPLES, and name the workflow interpreter [bl-cb63]
- Add harness-neutral adapter-wire types in src/provider/wire.rs [bl-bf79]
- Extract lernie-provider-anthropic to its own workspace crate [bl-f8bd]
- Add a --request <path> file-input flag to lernie-provider-anthropic (§4.4) [bl-becc]
- Pin the §4.4 non-streaming response shape to the Anthropic wire pass-through, with parser tests on the required fields [bl-2c54]
- Add a git-tree renderer for v0.2-shape repos [bl-8af1]
- Add the terminal compaction stub and the no-ff merge back [bl-8c66]
- Add a git-tree renderer for v0.1-shape repos [bl-209e]
- Scrub the branches.json nonexistence checks [bl-574a]
- Add the CLI outbound module, which execs a lernie subcommand [bl-02da]
- Add the filesystem watcher module (inotify with a polling fallback) [bl-ea1d]
- Retire branches.json for the single-source-of-truth principle [bl-4652]
- Add the lernie-ui-egui skeleton: workspace plus placeholder binary [bl-1013]
- Track unmerged branches in branches.json [bl-a764]
- Spawn the exchange branch with step commits [bl-e7e1]
- Define the harness and UI roles and the filesystem-as-event-stream contract [bl-bcd9]
- Retire the --endpoint argv for describe-driven endpoint discovery [bl-effb]
- Copy binaries to ~/.local/bin from the install target [bl-b64e]
- Add the lernie prompt subcommand: one committed exchange per invocation [bl-e048]
- Add the "everyone uses the front door" principle and §3.4, CLI as control plane [bl-dfb7]
- Extract the Anthropic adapter as the lernie-provider-anthropic binary [bl-7cfa]
- Define the provider-adapter contract and externalize the provider layer [bl-415c]
- Rewrite disk-first and symlink CLAUDE.md to AGENTS.md [bl-621d]
- Add the Anthropic Messages API client (blocking, non-streaming) [bl-9600]
- Add the lernie binary with a new subcommand for conversation scaffolding [bl-2904]
- Scaffold the conversation repo under template/ [bl-c91c]
- Promote "disk first" to the lead principle and fold in the bus framing [bl-4d73]
- Load and validate manifest, workflow, providers, agents, and version [bl-5bbc]
- Add the MIT LICENSE, the Cargo.toml license field, and a README note [bl-ffb8]
- Tighten the architecture spec: streaming, compaction, hooks, retry, principles [bl-2736]
- Revise the branch topology: dispatches branch, steps commit [bl-32fd]
- Align ARCHITECTURE.md to TAXONOMY.md terms [bl-cd2b]
- Add the Rust crate with a Makefile and pre-commit hook [bl-fe61]
- Add the .gitignore
- Initialize the repository
