---
name: read_tool_output
description: Read back the part of one of your own earlier tool results that was cut out of your context. When a tool writes more than the transcript budget allows, you are shown its first and last few kilobytes and a marker in the middle saying how many bytes were removed and giving you an address; hand that address to this tool and it returns the missing bytes, a page at a time, until the end. Use it instead of re-running an expensive or slow command, and instead of guessing at output you were shown only the ends of.
---

# read_tool_output

Every tool call's **full** output is captured on disk before anything is
cut. What you were shown in the result is a projection of that capture:
its first bytes, its last bytes, and a marker naming what was removed.
This tool reads the capture.

Nothing is lost when a result is truncated — only deferred. Re-running
the command is the wrong move: it costs time, it may have side effects,
and its output may not even be the same.

## Input

```json
{ "address": "steps/<agent-id>/<NNN>/tools/<tool-id>/output.json#stdout@2048" }
```

Copy the address **verbatim** out of the marker in the cut result. You
never construct one by hand. Its parts are the record the output was
captured in, the stream (`stdout` or `stderr`), and the byte to resume
at.

## Output

Standard output is the page: up to 4096 bytes of that stream, starting
at the address's offset, byte for byte as the tool wrote them. Nothing
of ours is mixed into it, so a page can be diffed or patched directly.

The line under `--- stderr ---` says where you are:

```
read_tool_output: bytes 2048-6144 of 31337 from steps/a/007/tools/tu_1/output.json stdout; continue with address steps/a/007/tools/tu_1/output.json#stdout@6144
```

Feed that next address straight back in to read on. The last page says
`end of stream` instead, and that is how you know you have all of it.

## When to use

- A result you need the middle of was truncated — read the middle
  instead of re-running the command.
- A long build, test run or log dump failed somewhere in the part you
  were not shown.
- You are about to issue the same command again with a filter you are
  guessing at. Read the real bytes first; then filter with knowledge.

## When not to use

- Reading a **file** — that is `read_file`, which takes a line range.
- Reading someone else's work — an address naming another agent's tool
  call is refused. You can page your own captures and only your own.
- Paging a whole 50 MB log into your context because you can. The
  middle of a build log is usually one line you could have grepped for
  with `bash`; the address is there when grep is not enough.

## Failure modes

- **Malformed address** → declined, naming the exact shape. Copy the
  whole string out of the marker, including the `#stream@offset` tail.
- **Another agent's address** → declined by name. Yours is the only
  namespace you can read.
- **No capture at that address** → the record is gone. Nothing to page;
  re-run the command.
- **Offset past the end** → declined with the stream's true size, so
  your next read is a correction rather than another guess.

Every result is a **result envelope**: an `Exit code: N` line first,
then the page, then the position line under a `--- stderr ---` marker.
