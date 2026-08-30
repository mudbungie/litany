# Task suite (ARCH §9.1, v0.9)

The evaluation task suite: manually constructed tasks with **machine-checkable
success criteria**, tagged by the failure category each is designed to provoke.
This directory is the single source of truth for what an experiment is measured
against (§9.3) — experiments and the suite version live in the repo together.

The runner that executes the suite (`agent-eval run --config <experiment> --suite
<suite> --runs N --agent <driver-cmd>`, §9.3 / v0.10) has **shipped** as the
separate crate `crates/agent-eval`. This directory is its input;
`crates/agent-eval/src/suite.rs` loads it and `tests/suite.rs` enforces its
well-formedness.

The harness driver `--agent` names has shipped too (`litany-eval-agent`,
`crates/litany-eval-agent`), so the suite runs end to end against a live
model: `make eval CONFIG=<experiment> SUITE=tests/suite RUNS=<n>
AGENT=litany-eval-agent` (see the repo README, "Run the suite"). The repo's
own gates still prove only well-formedness and runner logic (against a faked
agent) — a measured score is a property of a live run, never asserted by
tests.

## Layout

One YAML file per failure category, each a list under `tasks:`. A task's file
records its **primary** category; a task may carry **secondary** category tags
when it provokes more than one failure mode (this is how 50 tasks reach the
§9.1 target of ≥10 tasks per category across seven categories).

## Task schema

```yaml
tasks:
  - id: early-termination-01        # unique across the whole suite
    categories: [early_termination] # ≥1 of the seven tags below; first = primary
    prompt: |                       # the goal handed to the agent under test
      ...
    setup: |                        # optional: shell seeding the workspace
      ...                           #   (run before the agent; cwd = workspace)
    check: |                        # shell run in the workspace AFTER the run;
      ...                           #   exit 0 = pass, non-zero = fail
```

`setup` and `check` are ordinary shell. The runner (v0.10) seeds a fresh
workspace, runs `setup`, dispatches the agent with `prompt`, then runs `check`
in the workspace — **exit 0 is the sole pass signal**, so success is decided by
observable state, never by the agent's own claim. A task with no `setup` starts
from an empty workspace.

## Failure categories (§9.1)

The seven tags, each mapping to a §9.1 failure mode:

| Tag | §9.1 failure mode |
|---|---|
| `early_termination` | Early termination — stopping before the goal is met. |
| `scope_reduction` | Scope reduction — delivering a subset of the asked scope. |
| `skipped_tests` | Skipped tests — claiming done without running/passing tests. |
| `hallucinated_apis` | Hallucinated APIs or facts — inventing symbols that do not exist. |
| `error_recovery` | Error recovery failure — not recovering from a seeded broken state. |
| `fabricated_success` | Fabricated success claims — asserting success the artifact contradicts. |
| `context_hygiene` | Context hygiene — needle-in-haystack, compaction, prompt-injection resistance. |

## Metrics (§9.1)

The runner reports, per §9.1:

- **pass@1** (primary) — mean per-task pass rate over N runs (mean-of-means, N
  fixed per task), with 95% Wilson score intervals. Reliability.
- **pass@5** (secondary) — fraction of tasks passing at least one of five runs.
  Ceiling capability.

Optimization target is pass@1; pass@5 distinguishes capability shifts from
reliability shifts. Target baseline pass@1 on this suite is ~40% (§9.1, v0.9).
