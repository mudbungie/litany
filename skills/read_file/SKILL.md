---
name: read_file
description: Read a file at the given path and return its bytes verbatim, whole or by line range (`offset`/`limit`). Every result names the lines returned, the file's total, and the offset to continue at when more remain. Use when the conversation needs to inspect a specific file by path, before suggesting an edit or making claims about its contents. Files larger than 1 MiB are rejected; reach for `bash` (e.g. `head -n N`) for those.
---

# read_file

Reads a file from disk and returns its bytes. Best for source files,
configs, and short documents whose content the model needs to see in
full — and, with `offset`/`limit`, for reading a long one a window at a
time.

## Input

```json
{ "path": "<filesystem path>", "offset": 1, "limit": 200 }
```

`path` is a string. Relative paths resolve against your own worktree —
the branch checkout the harness runs every tool in (ARCH §3.3) — so
`messages/001-user.md` reads your own copy, not some other agent's.

`offset` (optional) is the 1-based line to begin at; omit it to start at
the first line. `limit` (optional) is how many lines to return; omit it
to read to the end. Both count from 1 — a zero in either is declined
rather than silently rounded up.

## Output

Raw bytes of the selected lines, carried in the `content` of the
matching `tool_result` block after the envelope's `Exit code: N` line
(below). Nothing of ours is prefixed to them: what you read is what the
file says, so a patch you author against it will match.

Under the `--- stderr ---` marker, every read — whole-file or ranged —
names what it returned:

```
read_file: 200 of 843 lines from offset 1; continue with offset 201
```

The continuation clause appears only when lines remain past the ones you
were given. A whole small file reads `read_file: 12 of 12 lines from
offset 1` and says nothing more.

## When to use

- The user references a file by name and you need its contents to
  reason about it.
- Verifying state on disk before suggesting an edit (cheaper and more
  reliable than asking the user to paste it).
- Working through a long file: read a window, then read the next one at
  the offset the previous result named.

## When not to use

- Files larger than 1 MiB — the tool rejects them with a `TooLarge`
  error rather than truncating, and `offset`/`limit` do not lift that:
  the cap is on the file, not on the window. Use `bash` with `head`,
  `tail`, or `sed` to scope the read.
- Directory listings or recursive searches — use `bash` (`ls`, `find`)
  instead.

## The `tool_output:` bound, and why the counts are here

Tool results are bounded per stream before they reach you (ARCH §3.3
*Bounded transcript projection*; 2 KiB of head and 2 KiB of tail by
default). A file longer than that is delivered with its middle replaced
by a marker — nothing is lost on disk, but the middle is not in front of
you. The stderr line above is what makes the recovery one call rather
than a guess: it names the file's line count, so `{"path": "…",
"offset": 201, "limit": 200}` reads the next window exactly.

## Failure modes

- Missing file or permission denied → exit non-zero, `is_error: true`,
  message of the form `open <path>: <io error>`.
- Oversize file → exit non-zero, `is_error: true`, message names the
  observed size and the cap.
- `offset: 0` or `limit: 0` → exit non-zero, `is_error: true`, message
  names which field and that lines count from 1.
- Malformed input JSON → exit non-zero, `is_error: true`.

An `offset` past the end of the file is **not** a failure: it returns no
bytes and a note reading `0 of <total> lines from offset <offset>`,
which tells you the file is shorter than you thought.

Every result is a §3.3 **result envelope**: an `Exit code: N` line
first, then the output described above, then — whenever the tool wrote
any, on success as well as failure — its stderr under a
`--- stderr ---` marker. So the exact reason for a decline reaches you
in the next step's request, and the stated code tells you which decline
it was.
