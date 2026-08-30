# Design: the engine crate becomes `litany` (bl-2f58)

**Status:** living document. Deliverable of bl-2f58. Records the census, the
one priced decision, and the acts left to an operator.

**Ruling being implemented (downstream, bl-37fd).** The harness becomes four
separately installed components meeting only at the wire:

- **yog** — the standalone server: holder of the world, the balls, the
  conversations. No UI, no execution.
- **lernie** — the **seat**: the window and the seat-client face, in its own
  crate and repo.
- **litany** — the **agent-loop engine**: *this* crate, named `lernie` until
  the fence.
- **thrall** — the **foot**: the tool host, advertise-and-execute only.

**The version fence.** The `lernie` name does not retire; it moves. The
engine's line under that name **ends at 0.0.x**; the seat's line under the same
name **begins at 0.1.0**. Both READMEs must state the fence, because the
crates.io record will carry two eras of one name and the fence is the only
disambiguation rule a reader gets. `litany` is held at 0.0.0 and continues the
engine's line.

**Terminology (AGENTS.md terminology discipline — this document introduces the
term).** **litany** — the agent-loop engine: the component that owns the
workspace substrate, the step loop, the tool executor and the prompt
assembly. It is a rename of an existing thing and introduces no new concept;
every term of art in `docs/TAXONOMY.md` and `docs/ARCHITECTURE.md` survives
unchanged.

---

## 1. Surface census

Tree-wide, case-insensitive `lernie`: **1990 occurrences across 510 tracked
files**. Grouped by what a rename actually costs, not by where the string sits:

### 1.1 Published identity — one edit each, no consequence beyond the registry

| Surface | Count | Note |
|---|---|---|
| `[package] name` / `[[bin]] name` / `repository` in `Cargo.toml` | 6 | plus `Cargo.lock` (2) |
| Binary path `src/bin/lernie/{main,cli}.rs` | 2 files | directory rename |
| `#[command(name = "lernie", …)]` (`src/cmd/mod.rs`) | 1 | the clap program name |
| Workspace member `crates/lernie-eval-agent/` | 4 files | internal, `publish = false` |
| `release-plz.toml` | 3 | names the internal eval crate only |
| `.github/workflows/release-plz.yml` | 8 | release-binaries artifact name, trusted-publisher prose |
| `.github/workflows/ci.yml` | 4 | CI git identity, `~/.cache/lernie/bz` test root |
| `tarpaulin.toml` | 6 | `exclude-files` globs + the symbol-name commentary |
| `Makefile` | 35 | `PATH_BINARIES`, install/verify/first-call targets, the two `*_HOME` derivations |

### 1.2 Durable state — the surfaces an existing install already has on disk

These are the only entries with a migration cost. Everything else in this
census is a text edit.

| Surface | Count | What exists on disk today |
|---|---|---|
| `LERNIE_HOME` | 153 | the override collapsing both roots (ARCH §2.2) |
| XDG subdir `lernie` (`src/harness_root.rs` `SUBDIR`) | 1 constant | `$XDG_CONFIG_HOME/lernie`, `$XDG_DATA_HOME/lernie` — `models.yaml`, `workflows/`, `workspaces/`, `skills/`, `tools/` |
| `refs/lernie/*` git namespace | 105 | **inside every workspace repo**: 8 kinds — `abandoned`, `budget-exhausted`, `conflicted`, `cwd`, `held`, `notify`, `retarget`, `returned` |
| `~/.cache/lernie/bz/<pin>` | 2 | CI/Makefile test root, regenerable |

The ref namespace has five constants (`src/workspace.rs` `MARK_REF_ROOT` and
`CONFLICTED_REF_PREFIX`, `src/prompt/workflow_actions.rs` ×2,
`src/prompt/inbox/deposit.rs`, `src/prompt/budget/mod.rs`) and ~100 literal
uses in tests.

### 1.3 Process-scoped env — set and read within one process tree, no migration

| Var | Count | Writer → reader |
|---|---|---|
| `LERNIE_CONV_REPO` | 39 | executor → tool subprocess (ARCH §3.3) |
| `LERNIE_CONV_BRANCH` | 55 | executor → tool subprocess |
| `LERNIE_LOCK_FD` | 20 | inbox baton, internal |
| `LERNIE_EXPERIMENT`, `LERNIE_EVAL_REPORT` | 21 + 16 | eval harness → eval agent, workspace-internal |
| `LERNIE_BIN` | 7 | one `make first-call` line |
| `LERNIE_CONFIG_HOME`, `LERNIE_DATA_HOME` | 4 + 3 | Makefile-local derivations |

**`LERNIE_CONV_REPO` / `LERNIE_CONV_BRANCH` are the exception inside the
exception.** They are process-scoped in *lifetime* but published in
*contract*: an operator-authored tool script under `<harness-root>/tools/`
reads them by name. Renaming them breaks such a script silently — it reads an
unset var and gets an empty path. That is not a state migration (nothing on
disk moves) but it is a compatibility break, and it belongs in the same
release note.

### 1.4 Prose and model-visible text — free, but not zero

| Surface | Count |
|---|---|
| `README.md` | 224 |
| `docs/*.md` (7 files: ARCHITECTURE, PRINCIPLES, TAXONOMY, USER_STORIES, and the three `DESIGN_*`) | 382 |
| `CHANGELOG.md` | 58 |
| `src/**` doc comments and prose | bulk of the 959 `src` hits |
| `schemas/tools/*.json` descriptions (9 files) | 9 |
| `skills/bash/SKILL.md` | 3 |
| `template/providers.yaml`, `install/models.yaml` | 3 |

`schemas/` and `skills/` text is **sent to the model verbatim** as tool
descriptions and skill bodies. Renaming there is correct and costs one cache
epoch, nothing more.

**`CHANGELOG.md` must NOT be rewritten.** It is the historical record of
releases published under the `lernie` name; rewriting it would erase the very
fence this document exists to state. The fence goes in as a new entry.

### 1.5 Downstream citations (read-only; the downstream's own edit)

The server's docs cite this crate by name and cite three of its documents by
path. Counts in the downstream tree:

| File | `lernie` hits |
|---|---|
| server `docs/DESIGN.md` | 504 |
| server `docs/STORIES.md` | 91 |
| server `docs/VISION.md` | 83 |
| server `docs/REMOTE.md` | 23 |

Documents cited **by path** from downstream prose: `docs/DESIGN_TOOL_INJECTION.md`
(server REMOTE §5 and the `src/tool_host.rs` module row of server DESIGN §12)
and `docs/DESIGN_MCP_BRIDGE.md` §6 (server REMOTE). Those paths do **not**
contain the string `lernie` and therefore survive the rename unbroken — only
the crate name in the surrounding sentence moves. `docs/ARCHITECTURE.md`,
`docs/TAXONOMY.md` and `docs/PRINCIPLES.md` are cited by section number
(`§N`), which is likewise stable.

Downstream env-var surface: `LERNIE_HOME` (19 doc + 94 source occurrences) and
`LERNIE_BINARY` (3 + 3). **`LERNIE_BINARY` is the downstream's own variable,
not this crate's** — this tree never reads it. Its rename is entirely
downstream's call.

---

## 2. The priced decision: `LERNIE_HOME`

### 2.1 What actually makes this expensive

The census above splits the env surface on one line: **does an outside party
supply the value?** For every var in §1.3 the answer is no — this process sets
it and this process tree reads it, so a rename is a same-commit edit with no
migration at all. `LERNIE_HOME` is the one var an outside party sets, and it
is the one whose *default* names a directory that already exists on disk.

So the real subject is not a string. It is three layers of standing state:

1. the variable name a caller exports;
2. the XDG subdirectory the default resolves to
   (`$XDG_CONFIG_HOME/lernie`, `$XDG_DATA_HOME/lernie`);
3. the `refs/lernie/*` mark namespace inside every workspace repository.

A decision that moves (1) and not (2) or (3) buys nothing — the divergence it
was meant to fix simply relocates.

### 2.2 Arm A — keep `LERNIE_HOME`

**Cost:** the variable name diverges from the crate name permanently. Under an
ordinary rename that is merely untidy: a stale name is a documentation
problem, and documentation problems are cheap.

**But this is not an ordinary rename, and that is the whole decision.** The
`lernie` name is not being retired — it is being **reassigned to a live
sibling component**, the seat. After the fence, `LERNIE_HOME` would be a
variable spelled after the seat crate that configures the engine's state root,
in a four-component system where all four are separately installed and the
seat is the component an operator interacts with first. The predictable
failure is not confusion in the abstract; it is an operator exporting
`LERNIE_HOME` expecting to relocate the *seat's* state and silently relocating
the *engine's* workspaces instead. Arm A does not preserve a stale name. It
manufactures a collision that does not exist today.

Arm A also leaves layer (2) unresolved. `$XDG_DATA_HOME/lernie` would be the
engine's data root while a seat crate named `lernie` ships alongside it and
has every reason to want that exact path.

### 2.3 Arm B — rename to `LITANY_HOME`

**Cost:** a state migration for every existing world. Concretely, and this is
the whole of it:

- **Every caller that exports the variable.** In this tree: the `Makefile`
  install/prime path and the e2e fixtures (all renamed in the same commit). In
  the downstream server: its world fold hands `LERNIE_HOME` down to every
  child it spawns, mapping its own `world/lernie` directory onto it — one
  constant, one layout row, one directory name.
- **The XDG default directories.** `~/.config/lernie` and
  `~/.local/share/lernie` become `~/.config/litany` and
  `~/.local/share/litany`. Two `mv`s per install. Of what sits inside them,
  `models.yaml` is hand-edited by contract and `workspaces/` is
  irreplaceable; `skills/`, `tools/` and `workflows/` are re-seeded by
  `prime`, which is seed-if-absent and idempotent (ARCH §2.2).
- **The `refs/lernie/*` namespace**, inside every existing workspace
  repository — a `for-each-ref` + `update-ref` + `update-ref -d` loop per
  repo, 8 ref kinds.
- **The downstream server's nested world directory**, `world/lernie` under its
  data root.

**What the migration is not.** It is not a data format change, not a schema
version, not a rewrite of any git history, and not a fallible operation: every
step is a rename of a path or a ref, reversible by renaming back.

### 2.4 The rejected middle: accept-both

Read `LITANY_HOME` first and fall back to `LERNIE_HOME` with a deprecation
notice; resolve the XDG subdir to `litany` if present else `lernie`. This is
the reflex, and it is wrong here for three reasons.

- It is **two representations of one fact** with a branch between them, in the
  one resolver that every verb calls. `docs/PRINCIPLES.md` names that smell;
  `src/harness_root.rs` is deliberately branch-free apart from the empty-value
  fallthrough.
- It **doubles the surface it was meant to reduce**: after the fence,
  `LERNIE_HOME` is a *live sibling's* name, so a fallback is not reading an
  old name, it is reading someone else's name.
- It is a **special case that is a missing reframe**. The general path is
  "resolve the engine's root"; the fallback exists only to avoid asking one
  operator to run two `mv`s.

### 2.5 RECOMMENDATION

**Rename. `LERNIE_HOME` → `LITANY_HOME`, `SUBDIR` `lernie` → `litany`,
`refs/lernie/*` → `refs/litany/*`, all three in the same commit, with no
compatibility shim and no fallback read.**

**Reasoning.** The two arms are not "clean name versus cheap migration",
because the `lernie` name is being reassigned rather than retired: keeping
`LERNIE_HOME` does not leave a stale name behind, it points the engine's state
root at a live sibling component's name in a system where all four components
are separately installed, and the failure that buys is an operator relocating
the wrong component's state without an error. Against that, the migration's
real population is small and enumerable — this crate's own line is at 0.0.x,
which is precisely the pre-stability window a break like this is supposed to
land in, and the only installs that exist are the operator's own plus the
downstream server's nested worlds, every one of which is a rename of two
directories and a ref loop, reversible, with no format change and nothing
irreplaceable outside `workspaces/`. Paying an enumerable one-time cost inside
the version fence that already exists for exactly this purpose is cheaper than
carrying a permanent name collision into a four-component system, and a
compatibility shim is worse than either arm because it makes the resolver read
a sibling's variable forever in order to save two `mv`s once.

**The one thing the rename must NOT take with it:** `LERNIE_CONV_REPO` and
`LERNIE_CONV_BRANCH` are a **published contract to operator-authored tool
scripts** (§1.3). They should still be renamed — the collision argument is the
same — but they are the one part of this change that can break third-party
code that this repo cannot see, so they need an explicit release-note line,
not a silent sweep.

### 2.6 The migration recipe — specified, NOT executed

Not run by this ball. Recorded so the operator act is one paste, not a
rediscovery.

```sh
# 1. Harness root (per install; skip either line if the source is absent)
mv "${XDG_CONFIG_HOME:-$HOME/.config}/lernie" "${XDG_CONFIG_HOME:-$HOME/.config}/litany"
mv "${XDG_DATA_HOME:-$HOME/.local/share}/lernie" "${XDG_DATA_HOME:-$HOME/.local/share}/litany"

# 2. Mark refs (per workspace repo under <data-root>/workspaces/*/repo.git)
for repo in "${XDG_DATA_HOME:-$HOME/.local/share}"/litany/workspaces/*/repo.git; do
  git -C "$repo" for-each-ref --format='%(refname) %(objectname)' refs/lernie/ |
  while read -r ref sha; do
    git -C "$repo" update-ref "refs/litany/${ref#refs/lernie/}" "$sha"
    git -C "$repo" update-ref -d "$ref"
  done
done

# 3. Anything exporting the old variable
#    LERNIE_HOME=... -> LITANY_HOME=...
```

The downstream server's nested world is the same two steps, rooted at its own
data root, plus renaming its `world/lernie` directory and the constant that
names it. That edit is the downstream's, in the ball that re-pins this crate.

---

## 3. Acts reserved to an operator

Ordered; none performed by this ball. Each is irreversible or crosses a
boundary this repo does not own.

1. **Adjudicate the fence.** Confirm the final `lernie`-named engine release
   (0.0.x) and its release note stating where the engine continued. This
   repo's `CHANGELOG.md` carries it; the note is the published record's only
   disambiguation between the two eras of the name.
2. **Publish `litany`.** The crates.io name is held at 0.0.0. Publishing is an
   operator adjudication — the rename landing on `main` would trip the release
   pipeline, which is why this ball does not land.
3. **Rename the GitHub repository** and update `repository =` and the
   trusted-publisher registration (owner + repository + workflow filename must
   all match, per the release workflow's own notes).
4. **Re-pin downstream.** The server bumps its dependency and sweeps its own
   citations — 701 name hits across four documents, plus its `LERNIE_HOME`
   fold and `world/lernie` layout row if §2.5 is adopted.
5. **Run the migration** (§2.6) on each install, once the new engine is the
   one being run.
