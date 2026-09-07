+++
title = "litany skills calls a never-loaded pool tool active and a never-loaded workspace skill unused on identical evidence"
created = 1788673910
updated = 1788744625
claimant = "Cantaloups-L5"
priority = 4
root_commit = "12899370c9ec7a5ed7f8e26d3d4fb914ea6c3310"
tags = ["usability-r1"]
+++
Scenario: lane KNOWLEDGE, round 1. Authored one workspace skill and then read the census:

    SKILL                OWNER      STATE   LAST USE  LAST PATCH
    apply_patch          pool       active  -         -
    bash                 pool       active  -         -
    cd                   pool       active  -         -
    ...
    tidewheel-decisions  workspace  unused  -         4 seconds ago

At the moment of that reading `bash` had been called several hundred times in the workspace and `tidewheel-decisions` zero times, so the verdicts happen to be right — but not for a reason the column supports. Both rows carry `LAST USE -`, and the STATE differs. STATE is derived from whether a `skills/<name>/` directory was ever added to a living agent branch, which only `load_skill` does; a pool built-in is granted, never loaded, so it can never have a LAST USE and can never read anything but `active`.

So for the nine pool rows the two columns are constants, and for a workspace skill they are the real signal. A reader cannot tell that from the table.

EXPECTED

Either say why a pool row cannot report use (a dash in STATE, or a distinct word), or report a pool tool's use from the transcript, where every call is committed and countable — the census's stated purpose is "oldest-used-first, never-used first", and today that ordering is meaningless for nine of its ten rows.

SEVERITY

p4: cosmetic until someone uses the census to decide what to archive, which is what DESIGN_LEARNING_LOOP §5 says it is for.