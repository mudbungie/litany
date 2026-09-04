---
name: search_history
description: Search this workspace's stored history for a fixed string and read back the matching entries, or recover one entry whole by its address. Every agent in the workspace is searched, not just you — including work that was compacted away, which stays reachable on the compactor's own branch. Use it before asking the user to repeat something, before re-deriving a decision that was already made, and whenever your own context has been compacted and you need the exact words rather than the summary.
---

# search_history

Searches the conversation history of this whole workspace — every
agent's stored transcript entries — and returns the matching entries
verbatim, plus an address for each that recovers it whole.

There is no separate archive and no index: the history *is* the
workspace's git repository, and this tool reads it. What it returns is
the bytes that were committed, never a summary of them.

## Input

Give **exactly one** of the two shapes. Both at once, or neither, is
declined.

```json
{ "pattern": "<fixed string>" }
```

Searches for `pattern` as a **fixed string** — not a regular expression,
no wildcards, no case folding. A hit is the moment an entry containing
that string was *added*, so an entry is found once no matter how many
branches later share it.

```json
{ "entry": "<commit>:<path>" }
```

Returns that one entry whole. The address comes from a previous
search's listing — copy it verbatim.

## Output

For a `pattern`, first every hit's address, newest first, one per line:

```
4f2a…c1:messages/007-claude-sonnet-5.json
9b30…7e:summary/002.md
```

Then the newest **five** of those entries, each framed by its address:

```
<entry address="4f2a…c1:messages/007-claude-sonnet-5.json">
…the entry's bytes…
</entry>
```

An entry longer than 8 KiB is cut to its first and last 4096 bytes, with
a marker in the middle stating what was cut and naming the address —
follow that address with `{ "entry": … }` to read the whole thing.

No hit at all is an empty listing. That is an answer, not a failure: the
string is not in this workspace's history.

## What is searched

- **The whole workspace**, not your own branch. A workspace is one
  concern, and each root agent in it is a past conversation — reaching
  across them is the point.
- **Only transcript entries** — `messages/` and `summary/`. Work
  products, goals, skills and config are not history; read those with
  `read_file`.
- **Compacted spans too.** When your context is compacted, the squashed
  entries stay reachable on the compactor's own branch, and that branch
  is inside what this searches. So "it was compacted away" is never a
  reason you cannot get the exact words back.

## When to use

- You are about to ask the user something they may already have told
  you, or told another agent in this workspace.
- Your context was compacted and the summary is not precise enough —
  search for the term and read the original entry.
- You need the decision *and its reasoning*, not your recollection of
  it.

## When not to use

- Searching files on disk — that is `bash` (`grep`) or `read_file`.
- Browsing. Every entry you pull in costs context; narrow the pattern
  instead of widening the read.

## Failure modes

- Both `pattern` and `entry`, or neither → declined, naming the two
  legal shapes.
- An `entry` address that names nothing → declined with git's own
  message. Check you copied the whole `<commit>:<path>`.

Every result is a **result envelope**: an `Exit code: N` line first,
then the output described above, then — whenever the tool wrote any, on
success as well as failure — its stderr under a `--- stderr ---` marker.
