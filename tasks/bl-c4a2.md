+++
title = "apply_patch accepts an Add with no body lines: a 0-byte file, an 'applied' receipt, and the name locked against the retry that would have written it"
created = 1788673661
updated = 1788673661
priority = 3
root_commit = "12899370c9ec7a5ed7f8e26d3d4fb914ea6c3310"
tags = ["usability-r1"]
+++
Round-1 daily lane. litany 0.0.10 under yog 0.0.38.

An `apply_patch` whose Add section carries no body lines is accepted, creates a zero-byte file, and answers with the applied receipt:

    input:  "*** Begin Patch\n*** Add File: REPORT.md\n*** End Patch"
    result: Exit code: 0
            {"status":"applied","files":[{"path":"REPORT.md","op":"add"}]}

    $ ls -l <worktree>/REPORT.md
    -rw-rw-r-- 1 ... 0 ... REPORT.md

The next `Add` of the same path then refuses — "file already exists; update it or delete it first" — so the empty add is not merely useless, it takes the name and locks the model out of the gesture that would have written the content. Twice in this lane a conversation ended with a 0-byte deliverable and a receipt saying it had been written.

A file with no content is never what an Add means. The section's whole payload is its `+` lines; zero of them is a malformed section, not an empty file, and `bl-fdbb` already established that this parser cares about exactly this boundary (a trailing blank line consumed as body).

Expected: an Add with no body lines is refused, naming the section — the same shape as the existing "add ...: file already exists" refusal. If a genuinely empty file must be creatable, it wants a spelling that says so.

Severity p3 on its own; it is the second half of two p1-shaped conversation failures in this lane, because a receipt that says `applied` is what stops the model retrying.