# Experiments (ARCH §9.3)

An **experiment** is a `workflow.yaml` variant — a config diff, no code
changes. Each lives in its own directory:

```
experiments/
└── <name>/
    └── workflow.yaml
```

The evaluation runner (`agent-eval run --config <name> --suite <suite> --runs
N`, ARCH §9.3 / v0.10) resolves `--config <name>` to
`experiments/<name>/workflow.yaml` and runs the suite (`tests/suite/`,
§9.1) against it N times per task, reporting per-task and per-category
pass@1 (with 95% Wilson intervals) and pass@5.

## Adding an experiment

Copy an existing `workflow.yaml`, edit the bindings (ARCH §6), and drop it
under a new `experiments/<name>/`. No code changes are needed — a new
experiment is deployable in under 60 seconds end-to-end (v0.10). The
`workflow.yaml` schema is the same one `litany` reads from a config commit
(ARCH §2.2); see `template/workflow.yaml` for the annotated reference.

## Shipped experiments

| Name | What it is |
|---|---|
| `baseline` | The default workflow: a **symlink** to `template/workflow.yaml`, the v0.9 baseline harness against which variants are measured (§9.1, ~40% ± 5% pass@1 target). |
| `single-attempt` | The default with the harness-owned retry loop disabled (`retry.max_attempts: 1`, ARCH §2.10/§6): isolates how much of baseline reliability the retry loop contributes. Also the repo's smallest non-empty config diff — the reference subject for the driver's experiment-application path (§9.3, bl-2e28). |

**The baseline is the template, not a copy of it.** An experiment is a diff
against the shipped default, so the baseline's diff is empty — and an empty
diff has nothing to store. `experiments/baseline/workflow.yaml` is therefore
a symlink to `template/workflow.yaml` (the same idiom as this repo's
`CLAUDE.md -> AGENTS.md`, bl-621d): one fact, one home, and `--config
baseline` resolves through it unchanged. A hand-copy would be a second home
for the default workflow, free to drift — which it had already begun to do
(both copies carried the dead `spawn_root_agent` verb bl-0e79 subtracted).
Editing the baseline is editing the default; that is a `template/` change,
and a variant of the default is a new directory here.

Variants that beat baseline on a failure category by a statistically
significant pass@1 margin are the v0.11 milestone (ARCH §12); they slot in
here as new directories.
