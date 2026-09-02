+++
title = "the built-in tool set is crate-private, so a host with a total injection must restate which names this engine can perform"
created = 1788236205
updated = 1788317799
claimant = "Tinker"
priority = 3
root_commit = "12899370c9ec7a5ed7f8e26d3d4fb914ea6c3310"
tags = ["seam"]
+++
`prompt::tool::builtin::NAMES` is the one list of what `litany tool <name>`
answers to, and it is crate-private. Nothing on the `cmd` surface says which
names this engine can perform, so a host that installs a `ToolInjection` — the
seam is total, it answers every tool call the agent makes — has no way to ask.

The consumer that needs it exists. yog's router routes a bare granted name to a
registered machine when one advertises it and consents, and otherwise re-enters
this engine's own front door, `<driver_target> tool <name>`, for the names the
engine implements. Deciding "the names the engine implements" is a question
only this crate can answer, so yog answers it by restating three string
literals of its own: `apply_patch`, `bash`, `read_file`. Add an eighth built-in
here and the restatement is stale in a way no gate on either side can see —
the new name simply refuses on a host with nothing enrolled, and the failure
looks like the host's.

It is the same fact `install/tests.rs::the_shipped_worker_grant_is_the_whole_tool_pool`
already holds in step *within* this crate: the seeded worker grant is the pool.
That test cannot reach across the crate boundary, and a host reading
`<data-root>/tools/*.json` off disk gets the schemas, not the answer — the pool
directory is seed-if-absent and an operator may have added entries to it that
no built-in stands behind.

## Shape

Re-export the list on `cmd` — the surface a host already imports `ToolInjection`,
`RoutedCall` and `RoutedCapture` from. A `pub const` or a `pub fn` returning an
owned `Vec<String>`; the module already renders the same list for humans
(`builtin::pool()`), so this is a second reader of one home, not a second home.

Nothing here changes behaviour, and a host that keeps its own list is not
broken by it — it just gains the option of asking instead.

## Gates

- tests: 100% coverage held; the export is exercised, and the human render and
  the exported list are pinned to the same source.
- docs: ARCH §3.3 names the built-in set as the third resolver hop's; say there
  that the set is readable on `cmd` by an injecting host.
- alignment: single source of truth (PRINCIPLES) is the whole argument, and
  narrow interface (§3.4 front door) is what the export must not widen — one
  list, no new verb, no new seam.