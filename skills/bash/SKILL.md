---
name: bash
description: "Run one shell command on this machine and read back what it printed. The shell is local: litany is a command-line program the user runs, and your command executes on that same machine — their filesystem, their network, their user account. There is no server, container, or remote sandbox between you and it, so a question about this host (its IP, its disk, its toolchain) is answerable by just running the command. It is not an interactive terminal and there is no prompt waiting for you: each tool call runs exactly one `sh -c '<command>'` with stdin closed and no TTY, waits for it to exit, and hands back its output. Shell state does not carry over — a `cd`, an `export`, or a shell function is gone by the next tool call, so chain what must share state into one command string (`cd sub && make`) or use the `cd` tool to move for real — but files the command writes stay. Every command starts in your current working directory, which is your worktree unless you moved it with the `cd` tool; what you write in your worktree is committed onto your branch, what you write outside it is not. The result always opens with a literal `Exit code: N` line — read it, since 1 (ran and failed), 127 (no such command) and 143 (cancelled) call for different next moves — followed by stdout, followed by stderr under a `--- stderr ---` marker whenever the command wrote any, on success as well as failure; no `2>&1` needed to see warnings from a command that succeeded. Use it for filesystem inspection (`ls`, `find`, `head`), text processing (`grep`, `sed`, `awk`), version-control queries (`git log`), builds and tests, and anything else a dedicated tool does not cover."
---

# bash

Executes one shell command and hands its output back. The de-facto
escape hatch when the conversation needs to do something a dedicated
tool does not cover.

## Input

```json
{ "command": "<shell command>" }
```

`command` is a string passed verbatim to `sh -c`. Quoting, expansion,
pipes, and redirection are the shell's responsibility.

## Where it runs

**On this machine.** litany is a command-line program someone runs; the
shell it spawns is a child of that process — same host, same user
account, same filesystem, same network route. It is not a hosted
service, not a container image, not a sandbox you were handed. `curl
ifconfig.me` reports the operator's own public IP because it *is* the
operator's network. Answer from it plainly rather than hedging about
"the tool's execution environment"; there is only one environment.

**In your current working directory.** The shell starts wherever you
currently are, so relative paths resolve there and `pwd` reports it. To
begin with that is your own worktree — the branch checkout the harness
materialized for you — and it stays that way unless you move with the
`cd` tool (ARCH §3.3). Anything you write, edit, or delete **under your
worktree** is committed onto your branch alongside the tool result, which
is how your work leaves the conversation as a product. Writes outside it
are real but uncommitted: off your branch, and invisible to whoever
dispatched you.

## It is not an interactive shell

There is no live prompt on the other end and no terminal session to
drive. One tool call is one `sh -c` process, spawned and reaped:

- **Stdin is `/dev/null`.** A command that reads stdin gets EOF
  immediately. There is no way to answer a prompt, feed input from a
  later tool call, or drive a REPL.
- **There is no TTY.** Programs that require one (`vim`, `less`,
  `top`, an `ssh` password prompt) fail or degrade; reach for their
  non-interactive flags instead (`git --no-pager`, `apt-get -y`).
- **The command runs to completion.** No wall-clock limit is imposed,
  so a slow build finishes — but nothing interrupts it from your side,
  and a command that blocks forever waiting on input blocks the step
  until the operator stops the agent.

## State between tool calls

The shell process does not survive the tool call, so **shell state does
not either**. `cd /tmp` in one tool call does not move the next one;
`export FOO=1`, `set -x`, shell functions, and aliases are all gone.
Chain them inside a single `command` string (`cd sub && make`) when you
need them together. To move for more than one call, use the **`cd`
tool** — that is the one piece of "shell state" the harness keeps for
you, because it is not shell state at all but a fact about you (ARCH
§3.3). What *does* persist besides it is the filesystem: files the
command creates or edits in your worktree are still there next time, and
are committed onto your branch.

## Output

The `content` of the matching `tool_result` block is the command's
**result envelope** (ARCH §3.3) — always the same three parts, in order:

```
Exit code: 0
<everything the command wrote to stdout>
--- stderr ---
<everything it wrote to stderr>
```

- **The exit code is always stated**, on the first line, on success as
  well as failure. Read it: `1` means the command ran and failed, `127`
  means it does not exist (a typo, or a tool that is not installed),
  `143` means the harness cancelled it. Those are three different next
  moves, and the `is_error` flag alone cannot tell them apart.
- **Stdout follows verbatim.**
- **Stderr follows under `--- stderr ---` whenever the command wrote
  any**, including on a zero exit — so compiler warnings, deprecation
  notices and progress logs from a *successful* command reach you. The
  marker line is absent when stderr was empty. You do not need `2>&1`
  to see stderr; use it only when you want the two interleaved in their
  original order, which separate capture cannot preserve.

`is_error` is set on any non-zero exit, but treat the stated exit code
as the authoritative fact — it survives every provider's wire format.

## When to use

- Listing or inspecting files and directories: `ls`, `find`, `tree`.
- Searching contents: `grep -r`, `rg`, `sed -n`.
- Slicing oversize files: `head -n 200 BIG_FILE` — the alternative
  for paths `read_file` rejects with `TooLarge`.
- Querying git: `git log --oneline -n 20`, `git status`.
- Running build / test commands when the conversation needs their
  output to reason about the next step.
- Answering a question about *this* machine — its IP, its disk, its
  installed toolchain. The answer is not hypothetical; go get it.

## When not to use

- Reading a single file in full: `read_file` is the cheaper, more
  predictable surface.
- Moving somewhere for several calls in a row: `cd` does it once,
  instead of a `cd X &&` prefix on every command.
- Anything interactive — there is no stdin and no TTY, so a command
  that expects either will hang or fail rather than prompt.
- **Waiting for another agent.** `sleep` is never how you wait for a
  dispatched child's result or a reply to a message: both arrive as
  deposits into your inbox, and a deposit revives a quiescent agent
  (ARCH §2.11). A sleep buys you nothing a step boundary does not buy
  better, and costs a model call to learn nothing. End your step
  instead (`dispatch` skill, *Waiting*). Sleeping to let a *local*
  process settle — a server you just backgrounded — is a different
  thing and is fine.

## Failure modes

- Non-zero exit from the command — `is_error: true`, the code stated on
  the envelope's first line, stderr under its marker.
- Malformed input JSON (missing `command`, wrong type, extra fields)
  → `is_error: true` with the message on stderr.
- SIGTERM cascade — when the *harness* is cancelled (`litany stop`,
  ARCH §2.9), SIGTERM is forwarded to the entire process group spawned
  by `sh`, then SIGKILL after the grace period. That is a
  cancellation, not a timeout: nothing kills a command for merely
  taking a long time. The reaped child's signal is reported as
  `128 + signo` (POSIX convention), so a clean cancel surfaces on the
  envelope's first line as `Exit code: 143`.

The tool's stdio contract follows ARCH §3.3 verbatim — the result
envelope states the exit code, carries stdout, and marks off stderr
whenever there is any, with `is_error` mirroring the exit code.
