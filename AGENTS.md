### Task Management
We use balls `bl` to do task tracking. Never commit directly on main. Invoke it for all task execution. Never ever make commits directly on main; all changes occur in a worktree and are merged in. Merges are always no-ff, to ensure that the merge is clean and representative.

When creating a task, always create the following gates:
- tests; ensure test coverage is at 100% and all tests pass. If there's something broken, you have to fix it before merge, no exceptions.
- docs: make sure the docs have been updated to represent the current state.
- alignment: check that the implementation is coherent against the project's spec docs — at minimum docs/ARCHITECTURE.md, docs/PRINCIPLES.md, and docs/TAXONOMY.md.

### Published text nobody committed

**No agent-session URL in this repository's published text, anywhere** (bl-1408, operator ruling 2026-08-30: *ban them, no reason to allow it*). An **agent-session URL** is a vendor console link to one recorded harness conversation, carrying that conversation's identifier in its path — "session" there is the vendor's product word inside this quoted term and inside the ported rule name `session-artifact`, never the interaction-span sense banned by `docs/ARCHITECTURE.md` §2.1.

Pull-request titles and bodies, issue text, review comments and release notes never carry one, nor any other conversation identifier. The harness convention of appending such a URL to a pull-request body is **overridden here**: strip it before you open the PR, because a body cannot be un-published afterwards — the forge keeps a body's edit history and serves `refs/pull/<n>/head` forever, so an edit buys the false assurance a history rewrite buys elsewhere. The ruling came from the seat repository, where exactly that happened and is now permanent.

The mechanical half is `scripts/leak-rules.sh`'s `session-artifact` rule, which since bl-1408 reads both forms — the bare id and the code-session URL path shape — so a commit message or a tracked file carrying one is refused at the moment of writing (README, *Pre-commit hook*). None of the text above is in any tree and no gate will ever see it; that half is yours.

### The disclosure gate's own regression half

`make leak-scan` runs `scripts/leak-scan.sh --self-test` before it runs the
scan, because a leak gate does not die by being wrong — it dies by silently
matching nothing after a pattern is edited, and then passing everything
forever. The self-test makes three promises. Every non-comment line of a rule's
fixture must be flagged *by that rule* and must carry the `notreal` marker,
while `clean.txt` / `clean-paths.txt` must be flagged by nothing (a gate that
cries wolf gets bypassed, and a bypassed gate is no gate). A fixture that does
not read as **text** in this locale is reported as an infrastructure fault in
its own sentence, never as a dead rule: `scan_rule` greps with `-I`, which
reports no hits for a file grep judges binary and says nothing about why, so
without that arm the box's fault and the gate's fault arrive as the same
sentence. And **a `grep -q` reads from a herestring, never from a pipe**
(bl-d3bc) — the self-test holds every tracked bash script under `scripts/` and
`.githooks/` to that, a foreign `#!` skipped (a POSIX `/bin/sh` script has
neither the option that makes the shape wrong nor the herestring that fixes it)
and a file with no `#!` in scope as a sourced fragment. A piped `grep -q` exits
the moment it matches and closes the read end; the writer dies of SIGPIPE
mid-write, and `pipefail` reports the pipeline failed *because the pattern
matched* (`PIPESTATUS` reads `141 0`). It flaked this self-test into calling a
live rule dead, at `scan_paths` — where the shape is `&& report` — it would
have dropped a real finding instead, and in `scripts/smoke.sh` it failed the
live run exactly when the transcript commit it looks for was there. The ban is
on the shape, not on the option, because a sourced file cannot see whether its
caller set `pipefail`; and enumerating **zero** scripts fails outright, because
a check that matches nothing is broken, never a clean tree. Measured on yog
bl-e33a, the original.

### Terminology discipline
Terms of art used in code, docs, prompts, or commit messages must have an explicit definition in `docs/TAXONOMY.md` or in the document introducing them (e.g. `docs/ARCHITECTURE.md` §2.1). Any undefined term of art requires user approval before use. When in doubt, check the taxonomy first, then ask — do not coin silently. Banned terms are listed in `docs/ARCHITECTURE.md` §2.1 (currently: bare "call", "turn", "session", "compression" in the context-management sense).
