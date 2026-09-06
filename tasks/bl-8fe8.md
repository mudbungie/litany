+++
title = "three conversations started in the same second are all minted the same name, and every seat verb then addresses none of them"
created = 1788673876
updated = 1788675717
claimant = "Cantaloups-L3"
priority = 2
root_commit = "12899370c9ec7a5ed7f8e26d3d4fb914ea6c3310"
tags = ["usability-r1"]
+++
Round 1, devadmin lane, driving yog 0.0.38 with litany 0.0.10 embedded.

Three `lernie start` calls on one workspace, back to back in one shell line:

    $ lernie start ops "reply with the single word ok"
    {"conversation":"ScarfPeach","kind":"started","ok":true}
    $ lernie start ops "reply with the single word fine"
    {"conversation":"ScarfPeach","kind":"started","ok":true}
    $ lernie start ops "reply with the single word yes"
    {"conversation":"ScarfPeach","kind":"started","ok":true}

Three distinct conversations, one name. From the roster, with their ids:

    ScarfPeach   20260906T035744Z-05f5097e   reply with the single word ok
    ScarfPeach   20260906T035744Z-0cee0f7e   reply with the single word fine

Same second in the id stamp, so the draw is seeded from a wall clock at
one-second resolution and `require_available` — which yog`s own
`src/names/mod.rs` note already calls a race ("beside the `require_available`
uniqueness check it races") — loses to it: three processes read the same seed
and each sees the name free.

It happened unprompted before it was ever probed: two admin tasks fired one
second apart in an ordinary session both came back `MeadowGelato`.

WHAT IT COSTS

The name is the handle every seat verb takes — `lernie transcript <ws>
<name>`, `follow`, `message`, `stop`, `agent`. With two live conversations
under one name the engine refuses:

    $ lernie transcript ops ScarfPeach
    {"error":"ambiguous conversation \"ScarfPeach\"","ok":false}

An honest refusal, and a dead end: the operator cannot address, read, message
or stop either conversation by the only handle the start gave them. The escape
is the raw agent id, which the start reply does not carry — it has to be
recovered from `lernie conversations`, matching on the goal preview.

EXPECTED: two conversations minted at once get two names. Whatever the mint
draws from must be per-creation rather than per-second — the agent id itself
is already unique and already in hand at mint time — or the availability check
must be the thing that decides, taken under a lock the racing creators share.

Severity p2: no data is lost and the id still addresses each one, but the name
a start hands back is not a handle you can rely on, and the failure is silent
at the moment it happens.

---

Mechanism located, and it is not in this repo. Filed as yog bl-d88f, which carries the repro, the fix and the failing test; this ball delivers the half that is litany's.

WHERE THE SEED COMES FROM

yog mints the conversation name itself at the fire and passes `--name <minted>` to `litany prompt`, so litany's own mint-on-omission never runs for a `lernie start`. The draw is `src/boundary/dispatch/doors.rs`:

    &SplitMix64::from_seed(
        seed.unwrap_or_else(|| crate::ui_state::content_hash(ts.as_bytes())),
    ),

`ts` is `Clock::stamp()`, which yog's `src/ui_state/clock.rs` defines as unix **seconds** as a string. One seed per second. `mint` is pure over the draw and the occupied set, and the occupied set is equal too — none of the racing fires has landed a dispatch commit, so `answer::names_in` reads the same living names for all three. Hence one name, exactly as the body reports.

litany's own paths are not affected: every creation site here draws from `SplitMix64::from_entropy` (nanoseconds XOR pid<<32), a grain finer than a creation.

WHY THE FIX CANNOT BE HERE

The body's second EXPECTED — "the availability check must be the thing that decides, taken under a lock the racing creators share" — was weighed and rejected. `require_available` answers by scanning `agents/*` for committed `name` blobs, so a lock that closed the window would have to span from the pre-flight to the dispatch commit, serializing conversation creation per workspace; and its outcome would be a REFUSAL of the second and third start, not the two names the ball asks for. That is worse than what is being fixed for as long as the seed stays coarse, and unnecessary once it is per-creation. The engine cannot enforce a contract on a consumer that supplies the generator — but it can state it, and had it stated it this would not have shipped.

WHAT LANDED HERE

The contract, in the two places a consumer reads it, plus a beat that pins the consequence:

- ARCHITECTURE §2.3 (the mint paragraph): "Purity is a contract on the caller. One draw plus one occupied set means one name, so a consumer that injects its own `Rng` owes it per-creation entropy — not per second, not per anything coarser than a creation. Nothing downstream can recover from a coarser grain: two creators racing inside it both scan the living names before either has committed a `name` blob, so their occupied sets are equal as well, and `require_available` — the check this mint is documented as racing — sees the name free for both."
- `src/workspace/agent_name/mint.rs` module docs, same fact at the mint itself; and `from_entropy`'s own note now says the nanoseconds and the pid are the load-bearing part rather than an implementation detail.
- `two_generators_seeded_from_one_wall_clock_second_mint_one_name` — written as the FAILURE rather than as the property. The property ("same seed, same name") was already pinned and reads as a guarantee, and reading it as one is what shipped the defect.

The residual after yog bl-d88f lands: with per-creation entropy the mint is a 1-in-292,140 collision per pair of simultaneous fires, because the occupied set cannot see a conversation that has not landed its dispatch commit. That is the documented race and it is tiny; the per-second seed made it a certainty.
