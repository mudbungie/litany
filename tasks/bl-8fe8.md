+++
title = "three conversations started in the same second are all minted the same name, and every seat verb then addresses none of them"
created = 1788673876
updated = 1788673876
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