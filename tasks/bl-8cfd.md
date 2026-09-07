+++
title = "a dev box's standalone litany and bz binaries have no CD and go stale behind the embedded ones"
created = 1788673941
updated = 1788745722
claimant = "Cantaloups-L5"
priority = 3
root_commit = "12899370c9ec7a5ed7f8e26d3d4fb914ea6c3310"
tags = ["usability-r1"]
+++
Operator ruling 2026-09-05: every device should run effectively full CD — any
new publication should result in an upgrade of the running versions.

After round 1 the four SERVICE components on a workstation reconcile themselves
hourly from the crates.io sparse index (yog bl-8ea9, thrall bl-6c98, lernie
bl-155a/bl-bae7). The two standalone CLI binaries do not, and one was measured
six releases behind on a live box: `litany` reporting 0.0.4 against a published
0.0.10, and `bz` at the published 0.0.14 only by luck of a recent hand install.

**The severity is p3 because nothing SERVES from them.** yog links litany,
balls and brazen in-process (yog DESIGN §16.7), so the versions that run agents
are the pins in yog Cargo.toml and they upgrade with yog. What goes stale is the
tool an operator or an agent reaches for at a prompt — and a CLI that is six
releases behind the library doing the same work in the same process is a
debugging surface that lies.

**The right answer is probably not a fourth copy of the reconciler.** There are
now three near-identical shell reconcilers in three repositories, each justified
by its own restart semantics: the engine defers to the §8.5 boundary, the foot
defers to an executing invocation, the seat has nothing to restart. A CLI has
nothing to restart either — it is exactly lernie's shape with the crate name
changed and the era fence dropped — which is the case for asking whether the
answer is a per-crate script at all, or one `cargo install-update`-class sweep
over a named set that a box states once.

What a design pass has to settle: which crates are on the list and where that
list lives (a box fact, not a repo fact, or it is a fleet-wide policy in the
wrong place); whether a CLI on a dev box should track releases at all when the
box also builds from a checkout (yog bl-6b27 is the sighting that two writers of
one install path need an explicit rule about which one wins); and whether the
answer belongs in any component repository rather than in the operator's own
tooling.