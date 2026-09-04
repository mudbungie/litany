---
name: python
description: "Run one python3 program on this machine and read back what it printed. The interpreter is local: litany is a command-line program the user runs, and your program executes on that same machine — their filesystem, their network, their user account — with `python3` resolved on that machine's PATH; there is no server, container, or remote sandbox between you and it. One tool call runs exactly one program, fed to `python3 -` with no time limit, starting in your current working directory (your worktree unless you moved it with the `cd` tool) and exiting when it exits; nothing carries over to the next call but the files it wrote. What makes it more than a shell is that your own tools are importable: `import litany_tools` gives one keyword-only function per tool you may call — generated for this program from your own toolset, parameters and docs from each tool's own definition — plus `litany_tools.invoke(name, arguments)` for any name. Every such call runs that tool exactly as a top-level tool call would: same permission, same review, same record. It returns a Result with `.stdout`, `.stderr`, `.exit_code` and `.ok` and raises only when the harness cannot be reached, so a tool that ran and failed is a value you branch on rather than an error that ends the program; `python` itself may not be called from a program (depth 1). **Only what your program prints on stdout comes back to you.** No inner tool result reaches you — not a line, not a tally — so loop, join, filter and count in the program and print the conclusion: dozens of tool calls cost you one result to read instead of dozens. Use it whenever the work is a loop, a fan-out, a join across several tools, or a filter over output far larger than the answer; use `bash` for a single command, and remember a program can write files with plain `open`."
---

# python

Runs one model-authored **program** — python3 source you write as the
`program` input — and hands back what it printed. It is the tool for
work that is a *composition*: many tool calls whose individual output
you do not need to read.

## Input

```json
{ "program": "import litany_tools\nprint('hi')\n" }
```

## Where it runs

**On this machine**, as `python3 -` with your source on stdin — the same
host, user account and filesystem `bash` gets. `python3` is assumed to be
on this machine's PATH, exactly as `bash` assumes `sh`; if it is not, the
result is `Exit code: 127` with stderr naming it, and the tool should not
have been granted in this deployment.

**In your current working directory** — your worktree unless you moved
with the `cd` tool, so relative paths resolve there. What you write under
your worktree is committed with the tool result; writes outside it are
real but off your branch. All of a program's inner tool calls ride that
same one commit.

**With no time limit.** Nothing kills a program for taking long. The two
things that end one are `litany stop` (the interpreter and every tool it
spawned fall to one signal) and the run's whole-tree budget.

## Calling your tools

```python
import litany_tools

result = litany_tools.bash(command="ls -1 src")
if result.ok:
    print(len(result.stdout.splitlines()), "files")
```

`litany_tools` is generated **for this one invocation** from the toolset
you actually have, so what it offers is what you may call. Each function
is keyword-only, its parameters are that tool's own schema properties
(required ones without a default), and its docstring is that tool's own
description. `litany_tools.invoke(name, arguments)` is the general path
the functions are sugar over — reach for it for a tool whose name is not
a python identifier.

A call returns a `Result`:

| Field | Meaning |
|---|---|
| `.stdout` | the tool's stdout |
| `.stderr` | the tool's stderr |
| `.exit_code` | the tool's exit code, stated by its own result envelope |
| `.ok` | `exit_code == 0` |

**A failing tool is a value, not an exception.** Only `DoorError` is
raised, and only when the harness itself could not be reached — so a
program that fans out over twenty files can count the failures instead of
dying on the first.

**Depth 1.** A program may not call `python`. Nesting buys nothing a loop
does not already give you, and the call is declined in place.

## What comes back to you

One result, for the `python` call itself:

```
Exit code: 0
<everything your program printed on stdout>
--- stderr ---
<everything it printed on stderr, a traceback included>
```

**No inner tool result appears anywhere in it.** Your program's stdout is
the entire record you will read of what happened, so print what you want
to know — and keep printing small, because everything you print costs
context. That is the trade the tool exists to make: dozens of
invocations, one result to read.

## When to use

- A loop over many files, paths or names, each needing a tool call.
- A fan-out whose individual results you would only be scanning for one
  fact — count them, filter them, print the fact.
- A join: read a file, decide from its contents which tool to call next,
  do it, and report the conclusion.
- Anything where the output you must read is far smaller than the output
  the tools would produce.

## When not to use

- One command: `bash` is one call and no interpreter.
- One file read or one patch: `read_file` and `apply_patch` are the tuned
  shapes, with typed declines.
- Waiting. A `time.sleep` is never how you wait for a dispatched child or
  a reply; both arrive as deposits that revive you at a step boundary.

## Failure modes

- The program raised — non-zero exit, traceback on stderr under its
  marker, `is_error` set. The last frames are what you act on.
- `python3` is not installed — `Exit code: 127`, stderr naming it.
- A tool you are not granted — the call comes back as a `Result` whose
  `.stdout` carries the harness's decline and whose `.ok` is false; the
  program keeps running.
- `litany stop` — the interpreter and every tool it spawned are ended
  together; the envelope reports the signal as `128 + signo`.
