# litany

[![Release-plz](https://github.com/mudbungie/litany/actions/workflows/release-plz.yml/badge.svg?branch=main)](https://github.com/mudbungie/litany/actions/workflows/release-plz.yml)

> **This crate was published as `lernie` through 0.0.x.** It is the same
> agent-loop engine, renamed. The `lernie` name did not retire — it passes to a
> sibling component, the **seat** (the window and its client face), at a
> **version fence**: the engine's line under the name `lernie` ends at 0.0.x,
> and a `lernie` release numbered **0.1.0 or above is the seat, not this
> engine**. That fence is the only rule that separates the two eras of the name
> on crates.io. If you are upgrading from `lernie` 0.0.x, read the migration in
> [`docs/DESIGN_ENGINE_RENAME.md`](docs/DESIGN_ENGINE_RENAME.md) §2.6 —
> `LERNIE_HOME` becomes `LITANY_HOME`, the XDG harness roots move from
> `.../lernie` to `.../litany`, and the in-workspace mark namespace moves from
> `refs/lernie/*` to `refs/litany/*`.

A git-backed agent harness. Design spec: [`docs/ARCHITECTURE.md`](docs/ARCHITECTURE.md).
Principles catalog: [`docs/PRINCIPLES.md`](docs/PRINCIPLES.md).
Vocabulary reference: [`docs/TAXONOMY.md`](docs/TAXONOMY.md).
Promise suite (the user stories 0.0.1 is evaluated against): [`docs/USER_STORIES.md`](docs/USER_STORIES.md).

CI runs `make ci` (`fmt-check` + `lint` + `coverage` with the 100% gate + `test-install`) on every push and pull request to `main`. The e2e tests exec the real provider adapter `bz`, which the test targets install themselves at the pinned version (see **[The pinned adapter under test](#the-pinned-adapter-under-test)**). The Rust toolchain is pinned in `rust-toolchain.toml` — CI, the pre-commit gate, and every contributor build under the same `rustc`/`rustfmt`/`clippy`. That pin binds this git checkout only; it is excluded from the published crate, whose supported floor is the declared `rust-version = "1.88"` (the crate's `let` chains, not edition 2024's 1.85).

## One command surface, two bindings

litany is defined once as a **command surface** — the set of verbs, their arguments, and their products (ARCH §3.4). It is consumable two ways, and both are the *same* control plane:

- **Exec binding** — run the `litany` binary: `exec("litany", args)` with env-var auth. This is what the CLI and every frontend use.
- **Linked binding** — depend on the `litany` crate and drive the same verb entries in-process. The crate's entire public API is `litany::cmd` (the `Cli`/`Command` clap surface, one `run` entry per verb, the `Fx`/`Outcome`/`Error` binding seam, and the `prelude` binding preludes). The linked binding is **pin-exact 0.x only** — no semver stability, the posture brazen takes toward litany.

Parity between the two is enforced mechanically, not by convention. `tests/command_surface_parity/` asserts the bijection at three depths: it pairs each verb's `Command` variant with its module's entry *as function values*, so the compiler — not an assertion — proves the two share one argument type and one product type; it walks the crate's whole module graph (via `syn`) and asserts that every externally reachable declaration (item, field, enum variant, method, derive, trait impl) is exactly a verb's entry, its arguments, its products, or the binding preludes, with every `src/**/*.rs` proven reachable so nothing can hide in a file the walk never opened; and it asserts, per verb, that the CLI's introspected argument set (via clap) is exactly that verb's public `Args` fields — same names, same arity, same named-vs-positional form. It rides `make check` (hence the pre-commit hook and GitHub Actions), so a divergence between the linked surface and the CLI fails the build.

## Quickstart

```
cargo install litany --locked                    # or: make install, from a clone
cargo install brazen --version =0.0.6 --locked   # the provider adapter, always needed
litany new ~/work/chat    # create a workspace (bare repo.git + config/default)
ANTHROPIC_API_KEY=... litany prompt ~/work/chat 'hello'
```

Three install routes, not one — see **[Install](#install)** for what each
lays down. Every route needs `bz`; only `make install` installs it.

## Install

There are four routes, and they do not lay down the same things. All
four need a second binary — the provider adapter `bz` — which only the
Makefile and image routes lay down for you.

| | `cargo install litany` | release tarball | `make install` | `make image` |
|---|---|---|---|---|
| binaries | `litany` | `litany` | `litany`, `agent-eval`, `litany-eval-agent` | `litany` |
| installs `bz` | no | no | yes, at the pin | yes, at the pin |
| runs `litany prime` | no | no | yes | no — it is your first act against the mounted roots |
| lands where | cargo's bin dir | wherever you unpack it | `$INSTALL_PREFIX/bin` | an OCI image, `$(IMAGE_NAME):<crate version>` |

### From crates.io

```
cargo install litany --locked
cargo install brazen --version =0.0.6 --locked   # the pinned provider adapter
litany prime                                     # found the harness root
```

You get the `litany` binary alone, in cargo's bin directory
(`~/.cargo/bin` unless `--root`/`CARGO_INSTALL_ROOT` says otherwise) —
no `agent-eval`, no `litany-eval-agent`, no `bz`, and nothing runs after
the build. The `litany prime` line is optional but explicit: `prime`
founds the harness root (below), and `litany new` founds it too on its
way to creating a workspace, so a user who skips `prime` is not stranded
— only uninformed about where their state went. Running it is how you
find out, because it says what it founded:

```
$ litany prime
litany prime: config root /home/u/.config/litany — models.yaml, workflows/
litany prime: data root /home/u/.local/share/litany — tools/, skills/, workspaces/
litany prime: harness root founded: 15 files seeded, 0 already present and left alone (seed-if-absent, ARCH §2.2)
```

That report is on **stderr** — `prime` has no stdout product (ARCH §3.4)
— and a re-run prints the same three lines with the counts swapped
(`0 files seeded, 15 already present`), which is how you tell an
already-founded root from a fresh one.

### From a GitHub release

Each `v*` release carries `litany-x86_64-unknown-linux-gnu.tar.gz`: the
`litany` binary, this README, and the license. Unpack it, put `litany`
on your `PATH`, then run the `cargo install brazen` and `litany prime`
lines above — the tarball ships no adapter and runs nothing.

### From a clone, with make

```
make install                                  # default: ~/.local/bin, XDG homes
make install INSTALL_PREFIX=/usr/local        # binaries -> /usr/local/bin/
make install LITANY_HOME=/opt/litany          # collapse both homes -> /opt/litany/
```

`make install` runs a release build and then:

1. Installs `litany`, `agent-eval`, and `litany-eval-agent` into
   `$INSTALL_PREFIX/bin` with `install -m 0755` (atomic overwrite, no
   symlinks). Make sure that directory is on your `PATH`.
2. Installs the provider adapter — brazen's `bz` — with
   `cargo install brazen --version =<pin> --locked`, where the pin is
   the `brazen = "=<pin>"` dependency in `Cargo.toml` — its one home;
   the Makefile and the load-time guard both derive from that line.
   One binary serves every provider
   (ARCH §4.4); the harness resolves `bz` on `PATH`, and a load-time
   guard rejects any `bz` whose version differs from the pin.
3. **Founds the harness root by invoking `litany prime`** — the single
   verb that seeds the installation substrate (ARCH §2.2), so the
   Makefile no longer duplicates the seeding. `prime` resolves the roots
   (XDG split, collapsed by `LITANY_HOME`) and lays down the default
   `models.yaml` under the **config root** (mechanism only: the optional
   `adapter:` override — no models, endpoints, or auth), the `tools/` and
   `skills/` pools and the `workspaces/` tree under the **data root**,
   and the empty `workflows/` templates dir. It is **seed-if-absent
   throughout**: a second run changes nothing, and a hand-edited
   `models.yaml` (or any operator-added pool entry) survives a re-install.
   The shipped assets are embedded in the binary, so `prime` needs no
   source tree — `LITANY_HOME=<dir> litany prime` seeds any fresh home.
   There is no profile pool: the config a workspace runs under is
   its own `config/default` commit, authored from
   [`template/`](template/) at `litany new` (ARCH §2.2).
4. Smoke-tests the freshly installed binaries with `litany --version`
   and a throwaway `litany new`. Failure aborts the install with a
   non-zero exit.

Its closing banner prints what the other two routes leave you to find
out: the install prefix, both harness roots, and the `bz` commands
below.

### The adapter is a second binary

Nothing prompts without `bz`. It is brazen's one stateless binary for
every provider (ARCH §4.4), it is **pinned exactly**, and litany refuses
a `bz` at any other version rather than downgrading silently:

```
cargo install brazen --version =0.0.6 --locked
```

The pin is not folklore you have to read this file for — the installed
binary carries it: `litany --version` prints the linked pin beside its
own version, `litany <version> (brazen 0.0.6)`. Its one home is the
`brazen = "=<pin>"` line in `Cargo.toml`;
the Makefile's `BRAZEN_PIN`, the load-time guard, `litany --version`,
and every pin printed in this file all derive from that line (a test
holds them equal). With no `bz` at all, the first verb that drives a
model call says so and hands you the command above.

Provider endpoints, auth, and wire dialects live entirely in brazen's
own config (`~/.config/brazen/config.toml`; inspect with
`bz --dump-config`, authenticate with `bz --login --provider <id>`).
litany references a provider *row* by name and never sees credential
material (ARCH §4.1).

### Where the state goes

The harness root is the installation-global substrate (ARCH §2.2), split
by XDG lifetime: `$XDG_CONFIG_HOME/litany` (hand-edited declarations —
`models.yaml`, `workflows/`) and `$XDG_DATA_HOME/litany` (machine-
populated pools and the `workspaces/` tree). `LITANY_HOME=<dir>`
collapses both to one directory, at install time and at runtime alike.
`litany prime` founds it, seed-if-absent throughout, so running it again
— or after an upgrade — never clobbers a hand edit. Only `make install`
runs `prime` for you; on the other two routes it is your first command,
or `litany new`'s side effect.

`make uninstall` removes the three installed binaries; `bz`
(installed via cargo) is removed with `cargo uninstall brazen`. The
harness homes (the config and data roots, holding config and
workspaces) stay put — clean them up manually if you want a true
uninstall.

### As a container image

`make image` builds an OCI image from `Containerfile` — a fourth route,
for a box that takes images rather than binaries. **The image is the
unit of install and nothing more.** No part of litany uses the container
filesystem as a feature, and no harness state lives in a layer: the XDG
roots are the runtime contract and they are mounted in.

```
make image                        # podman or docker, whichever is on PATH
make image CONTAINER_ENGINE=docker
```

It builds under the pinned toolchain (`rust:1.95.0-alpine`, checked
against `rust-toolchain.toml` during the build so the two pins cannot
drift), and copies two static-pie musl binaries into an `alpine` runtime
layer carrying `git`. About 30 MB.

**It ships `bz`, because a route that did not would not be an install
route.** The section above is explicit that nothing prompts without the
adapter; the image installs it from crates.io at the pin read out of
`Cargo.toml`'s `brazen = "="` line — the same one home the Makefile and
the load-time guard derive from.

**The runtime layer is what the engine execs**, which is why `FROM
scratch` is wrong here whatever the linking story says. Four programs,
and the reasoning is in the `Containerfile` beside each: `git` (the
harness is git-backed and shells to the binary on PATH for every
workspace act), `sh` (the `bash` built-in tool runs `sh -c`), `bz`, and
`litany` itself (the built-in tool set and dispatch re-exec it). System
CA roots ride along for the adapter's HTTPS and for an HTTPS git remote.
Past that list the layer is bare: a tool the harness is configured to
run that this layer does not have is a tool this box does not have. Add
it in a derived image rather than widening the base, so what the base
promises stays exactly those four.

`make image` **pushes nothing**, and there is no `push` target — the
same reasoning that keeps an irreversible act out of this Makefile's
reach. The registry is named (`ghcr.io/mudbungie/litany`, one package
per repo — yog `docs/DESIGN.md` §10.1, operator ruling 2026-08-30), and
the push still does not live here: it belongs to the release workflow at
tag time, where the publishing identity exists and nowhere else. What is
published is the version tag and the manifest digest, both immutable,
and never a moving `latest`. `make image` does apply `:latest`, but
**locally** — a local tag is a convenience on one box nobody else can
pull, where a published `latest` is a name whose bytes change under
everyone who ever wrote it down.

#### The image-side disclosure gate

That registry ruling is **conditional**, and `make image-scan` is the
condition. It runs as the last step of `make image`, so no image exists
on this box that has not been read.

**It is a second gate and not a reuse of the first.** `make leak-scan`
reads the git INDEX; an image is built from inputs no commit has — the
build context as the engine actually receives it, the base image's
layers, the package index, and the image CONFIG. A push is also less
recallable than a `cargo publish`: a tag can move, but the bytes anyone
pulled are theirs.

It reads three surfaces with the **same rule table** the commit gate
uses (`scripts/leak-rules.sh`, sourced and never copied):

- **The authored filesystem** — every file or symlink whose bytes differ
  from the pinned base image at that path. Both filesystems are exported
  and compared here, rather than diffing layer digests: it needs no JSON
  parser, it works on docker as well as podman, and it is the finer
  answer, since a file the build rewrote to identical bytes is not
  authored content.
- **The distro floor is accounted for, not exempted.** The runtime layer
  runs `apk add`, which adds thousands of files this repo did not write.
  apk's own ownership ledger says which package owns each one; a symlink
  resolving into that set is aliased distro content; everything else
  above the base is this repo's and is scanned. A path exemption would
  be an allowlist, and an allowlist is where a leak hides.
- **The image config** — every `Env`, `Label` and history entry. An
  `ENV` ships to everyone who pulls whether or not a file holds it, and
  build arguments echo into history.

The posture the commit gate already fixed carries over unchanged.
Findings **locate** and never reprint (truncated to twelve characters).
**Unreadable is rejected, not skipped**: the binaries this build authors
are `litany` and `bz`, and the expected set is DERIVED from the
Containerfile's `COPY --from=` destinations rather than typed into the
scanner — any *other* authored file the rules cannot read is a refusal.
And **both directions**, because a scan that has stopped matching passes
everything forever: `make image-scan` first builds a scratch image that
layers a fabricated secret into a file, another into an `ENV`, and an
undeclared binary beside them, and requires all three findings, before
scanning the real image.

What it cannot promise, stated rather than implied: it scans one image,
on the box that built it, before the push. It does not read what is
already in the registry, it cannot un-publish a digest, and whoever runs
the build can bypass it exactly as `--no-verify` bypasses the commit
hook.

#### What mounts where

`XDG_CONFIG_HOME` is set to `/config` and `XDG_DATA_HOME` to `/state`,
so the two harness roots named above are `/config/litany` and
`/state/litany`. The extra level is XDG's and not the image's: both
variables are parents of per-application roots by definition.
`LITANY_HOME=<dir>` still collapses both at run time for an operator who
would rather mount one directory.

```
podman run --rm \
  -v ~/litany-config:/config/litany:Z \
  -v ~/litany-state:/state/litany:Z \
  -v ~/work:/work:Z \
  litany:0.0.2 new /work/chat
```

Workspaces are named by path on the command line and can live anywhere.
The image asserts no location for them beyond a `/work` working
directory — whatever path is named has to be a mount if the workspace is
to outlive the container.

Nothing in the image runs `litany prime`. Seeding the harness root
writes fifteen files, and writing them into a **layer** would put the one
state litany owns where a mount cannot replace it and an upgrade cannot
see it. `prime` is seed-if-absent and stays the operator's first act
against the mounted roots — or `litany new`'s side effect, as on every
other route.

#### What the image deliberately does not contain

- **No harness root and no workspaces.** Both are mounts, for the reason
  just given.
- **No provider credentials.** Endpoints and auth live in brazen's own
  config, which litany never reads (ARCH §4.1); mount or inject it, and
  note that a credential baked into a layer is a credential published to
  everyone who can pull it.
- **No git identity.** `litany` commits into the workspaces it drives,
  and git will refuse with `Please tell me who you are` against an
  ambient-identity-less container. Supply one — `GIT_AUTHOR_NAME` /
  `GIT_AUTHOR_EMAIL` / `GIT_COMMITTER_NAME` / `GIT_COMMITTER_EMAIL`, or
  a mounted `.gitconfig`. It is not baked in because an identity in a
  layer signs every operator's commits with the same name.
- **No `cargo`, no compiler, no source tree, no `target/`.** The build
  stage is discarded whole; only the two binaries cross.
- **Neither `agent-eval` nor `litany-eval-agent`.** They are repo-side
  evaluation tooling that reads this tree's `tests/suite/` and has no
  meaning on a deployed box — the install table above already says the
  non-Makefile routes do not lay them down either.

### The macOS artifact

    make mac-artifact        # -> dist/aarch64-apple-darwin/{litany,bz}

`make mac-artifact` cross-produces the `aarch64-apple-darwin` binaries **from
the same Linux container line the image comes off** — the same digest-pinned
base, the same toolchain pin checked against `rust-toolchain.toml`, the same
`--locked` dependency answer the gate judged, and the same `brazen` pin read
out of `Cargo.toml`. So a mac binary is reproducible from the tree rather than
being whatever came out of somebody's laptop that afternoon.

**Both binaries**, for the reason the image ships both: `bz` is not optional,
and an artifact that was only the engine would be an install route that cannot
answer a prompt.

The product is **files, not an image**. The build's last stage is `FROM
scratch` carrying the two binaries; the wrapper is a fixture, is never pushed,
and is deleted when they have been lifted out. `make image-scan` therefore does
not apply to it and is not being skipped: the artifacts are compiled from the
same tree `make leak-scan` reads, exactly as the Linux release binaries are.

#### The toolchain is `zig cc`, and osxcross is refused

There were two ways to link a Mach-O binary on Linux, and the choice was made
on Apple's licence rather than on taste. osxcross drives **Apple's own SDK**,
which the *Xcode and Apple SDKs Agreement* forbids twice over — either clause
alone would settle it:

> **2.7** The grants set forth in this Agreement do not permit You to, and You
> agree not to, install, use or run the Apple Software or Apple Services on any
> non-Apple-branded computer or device, or to enable others to do so. … You
> agree not to rent, lease, lend, upload to or host on any website or server,
> sell, redistribute, or sublicense the Apple Software and Apple Services, in
> whole or in part, or to enable others to do so.

> **2.5** You may not alter the Apple Software or Services in any way in such
> copy, e.g., You are expressly prohibited from separately using the Apple SDKs
> or attempting to run any part of the Apple Software on non-Apple-branded
> hardware.

The first means the SDK may never sit in this repository nor in anything
published from it. The second means the usual escape — take the SDK path as a
build argument, keep it out of the tree, let the operator supply it — **does
not work either**, because the builder is not Apple-branded hardware. So this
repo does not hold the SDK at arm's length; it refuses the arm.

`zig` acquires nothing from Apple: it ships one darwin stub of its own,
`lib/libc/darwin/libSystem.tbd`, in its own distribution and under its own
licence. It is pinned by version **and** sha256; `cargo-zigbuild` (which
filters the darwin linker flags `zig cc` will not take) is pinned exactly and
installed `--locked`.

#### The limit, and the one edge it cost

zig ships libSystem and **no framework stubs at all**. A crate graph that links
only libSystem crosses cleanly; one that links any Apple framework fails at the
link step with *"unable to find framework"*, and there is no lawful way to
supply the frameworks on a Linux builder.

This graph had exactly one such edge. `chrono`'s `clock` feature is `now` plus
local-timezone detection, and the detection pulls `iana-time-zone`, which links
CoreFoundation on darwin. This crate uses `Utc` only — `src/prompt/clock.rs` is
the whole of the use — so the feature is now `now`, which dropped five crates
from the lockfile and ported the mac build by the same edit. That line in
`Cargo.toml` says so beside itself: widening it back to `clock` un-ports this
build.

#### What is proven, and what is not

There is no mac on the build box, so **the artifacts are never executed**. A
green build is not evidence: a wrong architecture, a dependency on a dylib no
stock mac carries, and a binary macOS would refuse to start all look identical
to a successful `cargo build`. `scripts/mac-verify.sh` reads each produced file
instead, on any platform, with no Apple tooling:

- **Proven** — 64-bit Mach-O, `arm64`, an executable; platform macOS with the
  minimum-OS and SDK versions it declares; every dynamic library it will ask
  for at load time, each of which must be a stock `/usr/lib` or
  `/System/Library` path; and that a code signature is present at all.
- **Not proven** — that they run. They have the shape of working mac binaries
  and have not been observed to be ones.

It runs **both directions**: five fabricated malformed inputs must be refused
before the real artifacts are read, because a checker that has quietly stopped
checking passes everything forever.

Two properties are worth knowing before an artifact is handed to anyone. **The
minimum macOS version is the pinned zig's, not a setting** — `rustc` asks for
one and this zig stamps its own — so read it off the artifact where
`mac-verify` prints it, never from a document. And **the signature is ad-hoc,
which is not notarization**: an arm64 mac refuses to start an unsigned binary
and the ad-hoc signature satisfies exactly that. A copy that arrives over a
network still carries a quarantine attribute, and clearing it — or replacing
the signature with a real one — is an act on a mac, by the operator.

## Configuration schemas

JSON Schemas for the harness-root and config-commit control files (per
[`docs/ARCHITECTURE.md`](docs/ARCHITECTURE.md) §2.2, §4.1) are generated
from the Rust types under `src/config/`. `make schemas` writes them to
`schemas/` for editor integration and external validators. Generation is a
golden test (`config::schemas::write_to` vs the checked-in `schemas/`):
`make schemas` runs it with `UPDATE_SCHEMAS=1` to rewrite the directory,
and the same test under `make check` fails if `schemas/` ever drifts from
the source types — so the tree is always current, with no separate binary
to run.

| File                          | Backed by Rust type                        | Config-commit / on-disk file          |
|-------------------------------|--------------------------------------------|---------------------------------------|
| `schemas/version.json`        | `config::version::Version`                 | `version` (config commit)             |
| `schemas/manifest.json`       | `config::manifest::Manifest`               | `manifest.yaml` (config commit)       |
| `schemas/workflow.json`       | `config::workflow::Workflow`               | `workflow.yaml` (config commit)       |
| `schemas/providers.json`      | `config::per_repo_providers::PerRepoProviders` | `providers.yaml` (config commit, `roles:`) |
| `schemas/models.json`         | `config::models::Models`                   | `<config-root>/models.yaml`           |

## Layout: harness root and workspaces

The harness root is installation-global state, split by XDG lifetime
into two homes (ARCH §2.2). `LITANY_HOME`, if set and non-empty,
collapses both to that one directory (test isolation, alternate
installs). Three distinct on-disk locations:

- **Config root** — hand-edited declarations, `$XDG_CONFIG_HOME/litany`
  (default `~/.config/litany`). Holds the global
  [`models.yaml`](docs/ARCHITECTURE.md#42-model-abstraction) (the
  optional `adapter:` binary override — §4.2; no model policy) and the
  `workflows/` templates. Provider endpoints and auth live in brazen's
  config, not here (§4.1); each role's model is named in a repo's
  `providers.yaml` (§4.3).
- **Data root** — machine-populated pools, `$XDG_DATA_HOME/litany`
  (default `~/.local/share/litany`). Holds the `tools/` and `skills/`
  pools plus the `workspaces/` tree. Shared across every workspace.
- **Workspace** — one git repository per workspace, at
  `<data-root>/workspaces/<workspace>/` (ARCH §2.2): a bare `repo.git`
  holding config branches (`config/<name>`) and agent refs
  (`agents/<agent-id>`) — **no `main`**. The control files
  (`providers.yaml` `roles:` only — §4.3, `manifest.yaml`,
  `workflow.yaml`, `version`, `souls/`) live in the **config commit**,
  read from each agent's governing config commit (`git merge-base`
  against the `config/*` heads — derived from ancestry, never stored).
  Agent worktrees are siblings under `agents/<agent-id>/`; `steps/` and
  `inbox/` sit at the workspace root, outside every worktree.
  Workspace repositories are never pushed to a remote.

`litany new` creates a workspace and authors its **first config commit**
— an orphan root on `config/default` — from [`template/`](template/),
the versioned skeleton embedded into the `litany` binary at build time:

```
litany new                     # auto-id under <data-root>/workspaces/
litany new /path/to/my-workspace
```

Or via the Makefile wrapper:

```
make new-workspace DEST=/path/to/my-workspace
```

The binary **founds the harness root first** — it runs the same
seed-if-absent routine `litany prime` is (ARCH §2.2), so a data root
nobody primed gains the `tools/` and `skills/` pools before they are
read, and a primed install is untouched (nothing is clobbered, no flag
is involved). That is what keeps the next step honest: the pools are an
input to the config commit, so an unprimed root would otherwise author a
commit with an empty `descriptions/**` and hand every agent forked off
it an empty toolset. It then runs
`git init --bare -b config/default <dest>/repo.git`,
materializes a transient authoring checkout, extracts the template's
control files into it, snapshots the data-root pools into
`descriptions/{tools,skills}/` (ARCH §3.3 descriptions-always), commits
(`config: init [config/default]`), and tears the checkout down. The
workspace is left with exactly one ref — the config commit every fresh
root agent forks off, and the lineage its resolution follows (§2.2). The destination must
either not exist or be an empty directory. With no path argument, the
destination is `<data-root>/workspaces/<auto-id>/`; the created path is
printed on stdout. `goal.md`, `soul.md` and `name` are intentionally not in the
template — they are written per-branch at dispatch time (ARCH §2.3,
§2.8), which also **removes the control files from the agent's tree**
(§2.2: control is read from the config commit; worktrees hold only
context).

**Pre-v1 clean break (ARCH §10):** the retired per-conversation layout
(a `root/` worktree with loose control files) is refused with an
actionable error, not migrated — create a fresh workspace with
`litany new`.

**First-run smoke test (required).** `litany new` authors the default
`providers.yaml` with a concrete model id, but validates it against
nothing — id validity is brazen's fact, and litany runs no model-list
reconciliation (ARCH §4.2, the settled stance). A wrong id surfaces only
at the first live model call. The required next step after creating a
workspace is therefore a live `litany prompt` (see the quick start
above): it is the cheapest — and, by that stance, only — check that the
authored id actually resolves on the wire.

`make smoke` automates exactly this:

```
make smoke     # scaffold a throwaway workspace + one live 'litany prompt'
```

It founds a throwaway harness root with `litany prime` — from the assets
**embedded in the binary**, the same front door `make install` uses, so
the shipped install path is exercised too — scaffolds a workspace with
`litany new`, then runs one live `litany prompt` against the **shipped
defaults** — worker role, provider `anthropic`, model `claude-sonnet-5` —
through the real `bz` data plane.
The verdict is read from **observable state, never the agent's own
claim**: the `litany prompt` exit code is 0, the agent ref
(`agents/<id>`) carries a committed transcript entry, and the off-worktree
step record (`steps/<id>/001/`) holds a response with **no wire error and
real assistant text**. That last pair is the point: an auth-failed run
still creates the branch and a step record whose response terminates in a
clean `end` — the failure rides an `error` event ahead of it — so
branch-exists and step-exists alone would pass a broken wire. `make smoke`
requires exit 0 **and** no `error` event **and** an assistant
`content_delta`.

By default `make smoke` runs the **shipped default** — provider
`anthropic`, model `claude-sonnet-5` — which needs a configured `bz`
credential for the `anthropic` provider (`bz --login --provider
anthropic`, or set `ANTHROPIC_API_KEY` / `BRAZEN_API_KEY`) and spends real
money. To run the same live check against **any other `bz` provider row**,
set **both** `SMOKE_PROVIDER` and `SMOKE_MODEL` (both-or-neither — one
alone is a usage error; unset leaves the shipped default byte-for-byte):

```
make smoke SMOKE_PROVIDER=local SMOKE_MODEL=<a-pulled-ollama-model>
make smoke SMOKE_PROVIDER=codex SMOKE_MODEL=gpt-5.4
```

The override is laid into the throwaway config root through the same front
door a real install uses — a `providers.yaml` override under
`<config-root>/template/` (the config-root override; the role assignment
is the whole model binding, ARCH §4.2/§4.3) — so there is no new `litany`
flag or verb. Local
`ollama` (bz's `local` provider row) needs no credential, only a model
that is actually pulled and served; the credential note above applies to
the `anthropic` default alone.

**What `SMOKE_PROVIDER=local` does and does not prove.** bz's `local`
row (protocol `ollama_chat`) rejects a canonical `tool_result` block:
the second step of any tool-using run comes back as
`{"type":"error","kind":"parse_input","message":"user accepts only text
content"}`. So the local recipe validates the **tool-free path only** —
one model call, assistant text, a committed transcript entry. It cannot
exercise a tool step, a compactor (whose whole toolset is
`write_summary`/`mark_for_deletion`), or any multi-step loop that runs a
tool. This is a brazen-side gap in that provider row, not a litany one,
and is filed there as brazen `bl-fba7`; to smoke a tool-using path,
point `SMOKE_PROVIDER`/`SMOKE_MODEL` at a row whose protocol carries
tool results.

`make smoke` is deliberately **not** part of `make check` or the close
gate: `make check` mocks the wire (httpmock Anthropic SSE), so it can
never catch a shipped default that fails on the real provider — which is
exactly how the fake id `claude-sonnet-4-7` once shipped unnoticed. It
runs only on demand.

## Authoring config commits

`litany new` authors a workspace's *first* config commit. Every later
one — the general harness-assisted user act of ARCH §2.2 — is
`litany config`:

```
litany config <workspace>                       # advance config/default
litany config <workspace> <name>                # advance config/<name>
litany config <workspace> <name> --from <src>   # fork config/<name> off config/<src>
litany config <workspace> <name> --orphan       # fresh orphan lineage
```

The verb materializes a transient checkout of the target config lineage,
refreshes the `descriptions/**` snapshot from the data-root pools (ARCH
§3.3), opens the checkout in `$EDITOR` (falling back to `vi`) so you edit
the control files (`workflow.yaml`, `providers.yaml`, `manifest.yaml`,
`souls/`, `version`), commits, and tears the checkout down. `<name>`
defaults to `default`. `--from` and `--orphan` are mutually exclusive and
only apply when creating a new branch. A `--from <src>` naming a lineage
the workspace does not have is resolved *before* the checkout is
materialized, and declined by name:

```
litany config: no config lineage "nosuch" in this workspace — existing lineages: default, strict
```

**Declining is fine, and leaves nothing behind.** Save no change and the
pass is *declined*: there is nothing to commit, so no commit is authored,
the branch does not move, and a `--from` / `--orphan` branch the pass
would have created is not left behind. That is a success — `litany
config` exits 0 and prints the one line

```
config/default unchanged: the edit changed nothing, so no config commit was authored
```

so empty stdout means a commit landed. The transient checkout is torn
down on every exit path (a decline, a git decline, an editor that fails),
so the next `litany config` always runs. Only a hard kill mid-pass can
leave the checkout behind, and the next pass clears it before starting
(ARCH §2.11 "the next touch heals") — at the cost of the killed pass's
unsaved edit, which was never committed.

This is the **only** act that advances a config branch (ARCH §2.3) —
and since bl-403b it *reaches running agents*: every agent on the
lineage resolves the new head at its next step boundary (§2.2
follow-the-tip; "configuration is changeable at any time, on any
turn"). Only a step already in flight finishes on the config it
started with.

A lineage you author this way is **startable by name**: `litany prompt
<ws> '<message>' --config <name>` forks the root off `config/<name>`'s
head instead of `config/default`'s, and the agent is governed by that
lineage (§2.2). A lineage the workspace does not have is declined by
name, with the pool that does exist.

## Moving a running agent onto a new config lineage: `litany retarget`

A same-lineage config edit needs no verb at all: resolution follows the
lineage's current tip at every step boundary (§2.2 follow-the-tip,
bl-403b), so fixing an expired model id on `config/default` reaches
every running conversation on it at its next step. What `litany
retarget` still does is change the **lineage** — move an agent onto a
different `config/*` line, or settle an agent held on its fork commit
because diverged lineages both reach it:

```
litany retarget <workspace> <agent>                 # onto config/default's head
litany retarget <workspace> <agent> --config strict # onto config/strict's head
```

It writes one ref, `refs/litany/retarget/<agent-id>`, at the target
config commit — and nothing else. **The agent's own executor lands it**
at its next step (ARCH §2.2, §2.3: no branch ever gains a second
writer), by re-forking the branch off that commit and replaying the
agent's own history on top: the same rebase-forward move the compaction
landing uses. Afterwards the ordinary ancestry query answers the new
config, with no new stored fact anywhere.

```
litany: [20260101-a1] marked for retarget onto a06b090c1d2e (config/default); it lands at the agent's next step (ARCH §2.2)
```

Three things worth knowing:

- **It takes effect at the next step, never mid-step.** A config governs
  steps. In practice you follow a retarget with `litany message`, which
  *is* that next step.
- **A target that already governs the agent is a clean no-op** — the verb
  says so and writes nothing.
- **Every refusal precedes the mark**, so a declined retarget leaves no
  debris: an unknown workspace, agent or lineage, or a target config
  whose `providers.yaml` grants the agent's role a tool its
  `descriptions/**` does not describe (ARCH §3.3), are all refused before
  the ref is written.

What is re-derived is everything config-shaped: the role's soul, the
`descriptions/**` cut to its grant, the control-file removal. The
agent's own facts — its goal, its name, its whole transcript and its work
products — are untouched.

## Switching a running agent's workflow: `litany workflow`

The workflow — the config's `workflow.yaml`, the named declaration of
what happens at every step (ARCH §6) — follows the lineage tip like
every other control fact, and carries the one per-agent override: the
engine operates by workflows, a workflow only determines the next step,
and it is consulted fresh at every step boundary, so switching one
agent is just changing which commit is consulted for it (ARCH §6 *The
workflow mark*):

```
litany workflow <workspace> <agent>                 # config/default's workflow
litany workflow <workspace> <agent> --config alt    # config/alt's workflow
litany workflow <workspace> <agent> --clear         # back to the governing config's
```

It writes one **standing** ref, `refs/litany/workflow/<agent-id>`, at
the named lineage's head — and nothing else. From the agent's next step
boundary on, that commit's `workflow.yaml` governs — bindings,
compaction clock, retry, budgets, tool-output bounds, tool control —
while the soul, providers, manifest and everything else stay with the
followed config. No re-fork, no rebase, no branch written. The mark
stands until re-marked or cleared — winning over the followed tip, so
it is also how one agent is held out of a lineage-wide change — and the
**nearest mark on the agent's descent wins**, so marking the root
switches its whole tree and a child's own mark overrides it.

The shipped default workflow has a name: the **basic agentic loop** —
`template/workflow.yaml`, the declaration every workspace's
`config/default` freezes at `litany new`. An unmarked agent runs it
exactly as before the mark existed; an A/B experiment is two config
lineages (`litany config <ws> alt --from default`, edit
`workflow.yaml`) and this verb to switch a live agent between them.

Every refusal precedes the mark: an unknown workspace, agent or
lineage, or a lineage head whose `version` or `workflow.yaml` does not
parse, is refused before the ref is written.

## Sending a prompt

```
litany prompt /path/to/my-conversation 'hello'
litany prompt /path/to/my-conversation 'hello' --name pale-otter
litany prompt /path/to/my-conversation 'hello' --config strict
litany prompt /path/to/my-conversation 'try again' --from <ref>
litany prompt /path/to/my-conversation 'hello' --pin AGENTS.md=./AGENTS.md
litany prompt /path/to/my-conversation 'survey it' --cwd /path/to/some/checkout
```

`--pin <dest>=<src>` (repeatable) freezes `<src>`'s exact bytes at
worktree-relative `<dest>` on the dispatch commit, beside `goal.md` and
`soul.md` (ARCH §2.5 caller-supplied pinned documents) — standing
context a caller pins without rewriting the goal or authoring a config
commit. Split is at the first `=`, so a source path may contain `=`; a
destination may not. The mechanism carries no filename policy — which
files count as project instructions is the caller's concern — but a
destination is validated before anything exists: it must be one
collision-free relative path, no `..`/absolute/`.git`, and no
harness-owned name (`goal.md`, `soul.md`, `name`, the control files,
`descriptions/`, `messages/`, `summary/`). Pins are ordinary blobs on
the dispatch commit (`git show agents/<id>:<dest>` is the provenance),
descendants inherit them by ordinary fork, and whether one composes
into assembled context is the governing manifest's §5.2 question — name
a destination its globs see. `litany dispatch` takes the identical
flag.

`--cwd <path>` starts the agent working in a directory you name instead
of its own worktree (ARCH §3.3 *Working directory*). It writes the same
working-directory mark the agent's own `cd` tool writes — once, before
the first step — so every tool call the agent makes runs there. The
path must exist and be a directory, and is refused before any branch or
ref exists, in the verb's own voice. Two things worth knowing: nothing
outside the worktree is committed, so work an agent does in a foreign
directory is real but off its branch (the same boundary `cd` has); and
**nothing is inherited** — a child of a `--cwd` agent is back in its own
worktree unless its own dispatch names a directory. `litany dispatch`
takes the identical flag; the model-facing `dispatch` *tool* does not.

`litany prompt` is the root-agent path (ARCH §2.3, §2.6, §2.7,
§2.8, §2.10). Each invocation spawns its own `agents/<conv-id>` branch
off the ref the start names (§2.2–§2.3 — there is no `main`),
drives each step's model call through brazen's `bz` (§4.4), and steps
until a terminal event. **There is no terminal compaction stage** (§2.7):
compaction runs only at the checkpoints `workflow.yaml` declares, and a
branch with no configured trigger never compacts. Merge-back is gone
(§2.6): the root branch persists on its own ref (§2.4), and an agent
returns by depositing a result message at the address its epitaph names
(§2.6):

1. Resolve the harness root (`LITANY_HOME`, else XDG homes, ARCH
   §2.2) and guard the workspace layout (a non-workspace, or the
   retired per-conversation layout, is refused — §2.2, §10). Load
   `<config-root>/models.yaml` (the optional `adapter:` override —
   §4.2) and, from the config commit's tree
   (`git show <config-commit>:providers.yaml`, §2.2),
   `providers.yaml` (`roles:` block — §4.3); the role's
   `{provider, model}` pointer is the whole model binding. Which config
   commit is **one derivation for both readings** (§2.2): the nearest
   `config/*` ancestor (`git merge-base` against the `config/*` heads —
   never stored) of the ref in hand. A fresh root asks it of the ref it
   is about to fork off — for the ordinary start that is
   `config/default`'s head, which answers itself; for a `--from` start
   it is that ref's own governing commit. `litany advance` asks it of
   an existing agent's branch. Either way that governing commit is then
   **followed to its lineage's current tip** (§2.2, bl-403b): exactly
   one config head standing over it is followed; diverged heads hold
   the agent on its fork commit, with a `litany: notice:` line saying
   so at every step until a retarget settles the lineage. Control is
   read from the followed commit — never the fork point's tree.
2. Run the load-time version guard: `bz --version` must equal the
   linked brazen crate version (§4.4). Under an `adapter:` override the
   guard is skipped and the in-band `MessageStart.v` handshake governs.
   Read the worker soul from the config commit's `souls/worker.md`
   (§2.2, §4.3).
3. Spawn branch `agents/<conv-id>` (§2.3 — the id is the bare
   hyphenated descent; the `agents/` prefix is the ref namespace) off
   the start's fork point — `config/default` by default, `config/<name>`
   under `--config`, any ref at all under `--from` (§2.3 *Any ref is a
   legal fork point*, §7.2 fork-from-history: a historical commit of any
   agent, a stopped tip, a config commit; provenance is the ancestry, no
   prefix marks a fork, and an absent one is declined before anything is
   created) — and allocate a worktree at
   `<workspace>/agents/<conv-id>/` (§2.2). Write the branch goal to
   `goal.md`, the role soul to `soul.md`, and any `--pin`ned documents
   at their destinations (below), remove the config commit's control
   files from the tree (§2.2 — the worktree holds only context), and
   commit — that commit's tree is step 1's read state (§2.10).
4. Build a typed `brazen::CanonicalRequest` (linked crate — the
   fail-open `extra` map stays unreachable), mirror it to
   `<workspace>/steps/<conv-id>/001/request.json` (a diagnostic
   artifact, outside every worktree, never read at runtime, §2.3).
5. **Model call, harness-owned retry loop (§2.10, §4.4).** Exec
   `bz --json --provider <row>` once per *attempt*, canonical request
   on stdin, appending each attempt's stdout verbatim to
   `<workspace>/steps/<conv-id>/<NNN>/response.json` as brazen `v=1`
   NDJSON — one self-delimiting segment per attempt, each ending in a
   terminal `end`. On a retryable in-band `Error`
   (`CanonicalError::retryable()`, never re-derived) the harness
   re-invokes `bz` with the identical request, up to the `workflow.yaml`
   attempt cap with exponential backoff — floored by the failed
   attempt's `Retry-After` pacing hint
   (`CanonicalError::retry_after_seconds`) when it carries one, so the
   config schedule governs and the provider's hint can only lengthen it
   (§4.4). brazen never retries; auth and
   endpoints are entirely its own. The `response.json` fd is held open
   across every attempt and backoff sleep — its close is the §3.5
   IN_CLOSE_WRITE completion signal. As the events stream, the harness
   tracks only their *framing* — the terminal `end`, an in-band `Error`,
   the handshake `v` — for retry/classification; `meta.json` carries
   `{commit, started_at, ended_at}`. The events' *content* streams into
   the **transcript writer**'s (§2.3) staging file
   `<workspace>/steps/<conv-id>/<NNN>/staging.json`,
   appending each content block as it completes; segment authority
   (§4.4) truncates it on an `Error` attempt and the settling `Finish`
   seals it — one stream, two sinks (diagnostic `response.json` +
   transcript), never read back. When the model call completes, the
   sealed file is renamed into the worktree as
   `messages/NNN-<model-id>.json` — its origin token is the model that
   authored it (§2.3), the body an API-shaped object carrying the
   canonical `Content` blocks under `content` plus the provider's token
   `usage` beside them (§2.3 *Usage rides the entry*; a bare block array
   with no usage is equally lawful and still parses) — and committed. `NNN` is the branch's transcript counter,
   max-present-plus-one from the `messages/` listing, evaluated at
   commit time. The initial user message now enters through the front
   door like any other (§2.11): the executor deposits it into the agent's
   own inbox, and the step-boundary drain delivers it as the first
   transcript entry `messages/NNN-user.md` (bl-1129) — no bespoke
   initial-message path beside the drain.
6. **Step loop (§2.5).** At each step boundary the executor first
   **drains the inbox** (bl-1129, §2.11): after committing any
   renamed-but-uncommitted stray a prior death left in `messages/`, it
   moves each pending `inbox/<agent-id>/<sender>-<NNN>.md` into the
   worktree as `messages/<counterNNN>-<sender>.md` (a literal `rename(2)`
   — one home at every instant) and commits the move, in a deterministic
   `(mtime, filename)` order, ahead of the read-state capture so a
   delivered message is part of the commit the model call assembles from.
   Each step then re-assembles its model-facing history
   from the read-state commit's tree — `readdir` of `messages/`, sorted
   by the filename's `NNN` prefix, each entry composed by its origin
   token (`NNN-<sender>.md` → user text, `NNN-<model-id>.json` → the
   assistant message — any `.json` token but the reserved `tool`,
   `NNN-tool.json` → `tool_result` in the following
   user message), with consecutive same-side entries grouped into one
   alternating wire message. There is no in-memory history and no
   git-log walk; running, retry, and replay are one code path against
   one input, the commit's tree (§2.3, §5). If the settled model-output
   entry carries any `tool_use` block, run every one through the tool
   executor — the per-tool-call records land under
   `<workspace>/steps/<conv-id>/<NNN>/tools/<tool-id>/` (out of every
   worktree, §3.3; written but never read at runtime), and as each tool
   resolves the transcript writer commits `messages/NNN-tool.json` (its
   canonical `tool_result` block) — then loop into step `<NNN+1>`. A
   step with no `tool_use` block is terminal. Step ≥2 has no *dispatch*
   commit, but each step's transcript entries (assistant output, tool
   results) do advance the branch tip, which is that step's read state
   (§2.10). `tool_use`/`tool_result` pairing holds by construction: a
   tool result commits immediately after its emitting step's model-output
   entry, so it always lands in the immediately following user message.
   Closing each tool step, the executor reads the **compaction
   checkpoint clock** (§2.7, §6) — `compaction.intermediate.trigger` in
   `workflow.yaml`: `every_n_commits`, `every_t_seconds`, or the
   agent-elected `on_flush`, all derived from git (commits and elapsed
   seconds since **this branch's own founding commit** — its dispatch
   commit, or its last compaction base if that is newer; never a stored
   counter, and never the inherited history a fork brings with it). A
   compactor is excluded from the eligible set outright: it *is* the
   compaction, not a subject of one. So is a branch whose **last
   checkpoint has not answered yet** — a compactor it dispatched that
   carries no returned mark — because the clock measures from a landing
   and a dispatched pass has landed nothing, so without that the next
   boundary would fire the same checkpoint again and pay for a second
   full model loop over the same span. When it is due, the
   `worker_flush: dispatch(compactor)` binding forks a compactor off the
   **compaction point** — the branch tip, or `HEAD~keep_recent` behind
   it when the config retains a recent tail (§2.6, §6) — and the branch
   keeps stepping straight through it; no quiescence is imposed. Omit
   the `compaction:` block and the branch never compacts. Should two
   passes ever race to the same `summary/<NNN>.md` anyway, the **late
   lander is refused**: the first landing rebases its compaction point
   away, the second cannot prove its own point is still reachable, and
   it is superseded — nothing is overwritten and nothing is versioned.
7. **Terminal return (§2.6, §2.3 step 5).** Every terminal event —
   normal completion (`final-response`), budget exhaustion
   (`budget-exhausted`, §6), and stop (`stopped`, §2.9 — the executor's
   SIGTERM handler deposits on its way out) — deposits a **result
   message**: an ordinary deposit whose frontmatter adds `epitaph:`
   and `terminal_ref:` (the branch tip) and whose body is the terminal
   response iff the agent spoke. **The epitaph picks the inbox** (§2.6):
   a `final-response` **reply** answers whoever last prompted this agent
   — its own transcript's newest delivered message, which for the
   dispatch step is the dispatcher — while a `stopped` /
   `budget-exhausted` / `died` **obituary** goes to the dispatcher
   whoever prompted last. A reply whose last prompter is the user
   addresses nobody: it is read in this agent's own conversation, which
   is also the ordinary root case (§2.4). The deposit is executor-side,
   never a model tool call ("Return is not a verb"). At delivery **in the
   dispatcher's inbox**, a result message applies the
   fork-point→terminal **work-product transfer** as one commit before its
   delivery commit, filtered to work products; a diff that fails to apply
   is declined at `refs/litany/conflicted/<agent-id>` (§2.6). A reply
   delivered anywhere else is an ordinary message — the transfer is
   defined against the fork the dispatcher made and nobody else's.
8. **Exit protocol (§2.11).** With the terminal deposit landed, the
   executor runs the branch's terminal `workflow.yaml` bindings
   (`branch_stopped` → `mark_abandoned` / `notify_ui`, §6), releases the
   executor lock, and only then spawns a driver at its own agent and —
   the deposit's own probe-and-launch — at the parent the deposit just
   revived. Both launches are fire-and-forget and both are decided by
   epitaph *value*: a final response launches, `stopped` and
   `budget-exhausted` never do. **No terminal compactor is dispatched**
   (§2.7): the v0.3 terminal-compaction stage is deleted, along with the
   `Dispatcher` re-entry that existed only to run it. Compaction is a
   checkpoint event (step 6), never an exit stage. **Merge-back is gone
   (§2.6):** the root branch persists on its own ref (§2.4); nothing
   merges back, and the agent's worktree is not torn down (quiescence,
   not teardown, §2.3 step 6).
9. Print the agent id (the bare conv-id) on stdout.

After `litany prompt` returns, inspect the agent against the bare
workspace repository:

```
cd /path/to/my-workspace
git -C repo.git log --oneline --decorate agents/<conv-id> -4
git -C repo.git ls-tree --name-only agents/<conv-id> messages/
git -C repo.git show "agents/<conv-id>:messages/002-<model-id>.json"
ls steps/<conv-id>/
```

The log is the dispatch commit followed by one `transcript NNN:` commit
per entry, its subject naming that entry's **origin token** — `user`, a
sender's agent id, `tool`, or the authoring model's id:

```
f265de7 (agents/…) transcript 002: qwen3.5:9b […]
7ae527e transcript 001: user […]
f643a50 step 001: dispatch […]
6f4bd05 (config/default) config: init [config/default]
```

`ls-tree` lists the transcript itself (`messages/001-user.md`,
`messages/002-<model-id>.json`, …) and `show` prints one entry — a
model-output entry wraps the canonical `Content` blocks in `content` and
states the provider's token counts beside them, e.g.
`{"content":[{"type":"text","text":"pong"}],"usage":{"input_tokens":812,"output_tokens":3}}`,
so token counts read off the committed bytes with no `steps/` lookup
(§2.3 *Usage rides the entry*). A bare `[{"type":"text",…}]` array — every
tool entry, and every model entry written before usage rode along — is
equally lawful. `ls steps/<conv-id>/` lists the
off-worktree step records, one numbered directory per step, each holding
`request.json`, `response.json`, and `meta.json` — plus, beside them,
`driver.log`: the stderr of every detached driver launched for this
agent, appended across launches, which is where a driver's declines are
read (ARCH §2.11 — a `setsid` driver has no terminal to print to).
Every one of those declines is prefixed **`litany: notice: `** — a
compaction landing declined or superseded, a budget stop, a launch that
fell into the accepted crash class, a retarget decline. That prefix is
the contract for a program capturing this file: a line carrying it
states what the harness declined or stepped past, and the process
carried on with its exit code untouched; a line without it, on a
driver's stream, is the process dying. Grep for it rather than for the
sentence after it, which is free prose and gets reworded (ARCH §2.11).
There is **no
`summary/`** on a branch that never reached a compaction checkpoint
(step 6) — and no merge commit ever: once a compactor has returned, what
appears is a single-parent `compaction base [<compactor-id>]` commit
carrying the summary, with the compacted span squashed behind it and the
later commits replayed on top (§2.6 rebase-forward).

The root branch persists unmerged by design (§2.4), so the health metric
is no longer branch count but silent deaths and undelivered returns
(ARCH §8) — read straight from git refs, the executor lock, and inbox
listings, with no sidecar file.

## Stopping a conversation

```
litany stop /path/to/my-conversation <conv-id> [--stop-children]
```

Sends `SIGTERM` to the process group of the **one executor** driving
`<conv-id>`, with a 5-second flush deadline before `SIGKILL`. This is the
same cascade pattern adapter (§4.4) and tool (§3.3) cancellation use,
applied to the harness itself
([ARCH §2.9](docs/ARCHITECTURE.md#29-stopped-branches)). The group signal
reaches that executor's own `bz` and tool subprocesses — its limbs — and
**stops at the agent boundary**: a dispatched child harness has taken its
own process group, so a bare stop does not fell it. A running child
outlives the stopped parent and revives it later by depositing its result
(§2.11) — stopping a parent strands nothing.

`--stop-children` opts into the agent→agent cascade: it walks the id
namespace — the descendants of `<conv-id>` are exactly the inbox
directories prefixed `<conv-id>-` (§2.3), one prefix scan reaching every
depth — and folds each descendant executor's group into the same sweep.
The pid is discovered by scanning `/proc/<pid>/fd/*` for the process
holding the agent's inbox-directory lock fd open — the executor lock
(§2.11), held for the whole step loop, so a stop lands even during tool
execution when no `response.json` is open — no sidecar pid file. Linux
only.

The pgid that scan produces is **vetted before anything is signalled**,
because a pid is discovered before its group has settled: between a
driver's fork and the `setpgid`/`setsid` it runs at startup, `/proc`
still reports the group it inherited from its spawner — your shell job.
So a pgid is trusted only once it equals the holder's own pid (a group
leader's does, and every driver becomes one), re-read a bounded number of
times while it does not, and refused rather than signalled if it never
settles; a stop that signals nothing is re-runnable, one that signals
your shell is not. `litany stop` additionally refuses any group it is
itself standing in (§2.9).

The group signal reaches every member independently: `bz` installs no
handler and dies at once (leaving the missing-`end` signature, §4.4),
while the **executor catches its own copy** — SIGTERM is catchable — and,
instead of dying on the spot, deposits its branch's `stopped` result on
its way out (§2.9 step 3, executor-side, "Return is not a verb") and then
exits cleanly. Catching shields nobody: the kernel already delivered to
`bz` and the tools. For a root the deposit is a no-op (no parent inbox);
the observable is the clean exit.

Because the model call is where the wall time goes, that is where a stop
usually lands — so the clean exit is the ordinary case, not the rare one.
The **flag classifies, not the error's shape**: a kill lands wherever the
adapter was, leaving a half-stream, a torn JSON line, or a provider error
depending on the instant, and with a stop pending each is read as the
stop. With no stop pending the same faults still propagate non-zero, so a
genuinely dying adapter is never hidden. The retry loop respects the flag
too: a stop is never followed by another `bz` invocation.

Behavior:

- **Idempotent.** A branch with no live writer (already stopped, or the
  harness exited cleanly) returns success without sending any signal.
- **Errors when** the agent branch (`agents/<conv-id>`) doesn't exist.
  Surfaces as a non-zero exit with a `litany stop:` prefix on stderr.
  (The old "already merged" refusal died with `main`: nothing merges,
  so there is no merged state to refuse — an already-terminal branch is
  simply the idempotent no-holder case above.)
- **No on-disk cancel marker.** The §2.9 signature of a stopped branch
  is the latest step's `response.json` closed without a terminal `end`
  event — produced by `bz` dying mid-stream on its own SIGTERM (§4.4);
  the executor's `stopped` deposit is an independent write to the inbox
  tree and never touches that signature.

The frontend's stop button (per [ARCH §3.5](docs/ARCHITECTURE.md#35-ui-contract))
exec's this exact subcommand; there is no second control surface.

## Built-in tools (v0.3, +v0.4 Phase 2 dispatch)

The agent can call **built-in tools** that ship inside the `litany`
binary as `litany tool <name>` subcommands (ARCH §3.3 / §12). The tool
executor's resolution order — `<data-root>/tools/litany-tool-<name>`
→ `PATH` → `litany tool <name>` — falls through to this in-process
route for tools not externalized.

Each built-in is the triple §3.3 pins:

- **Binary** — the `litany tool <name>` subcommand. Reads
  `tool_use.input` JSON from stdin, writes raw bytes to stdout, exits
  0 on success or non-zero on failure; the harness renders the three
  into the §3.3 *result envelope* that becomes `tool_result.content`,
  each stream first head+tail-bounded per the workflow's `tool_output:`
  block (§3.3 *bounded transcript projection*, bl-d5fa) — the cut
  middle is replaced by a marker naming the original byte/line counts
  and the diagnostic `output.json` that keeps every byte.
- **JSON schema** — at [`schemas/tools/<name>.json`](schemas/tools/),
  seeded to `<data-root>/tools/<name>.json` by `litany prime` (which
  `make install` invokes, ARCH §2.2). Sent verbatim as the
  `input_schema` of the tool's entry in the model call's `tools: [...]`
  array.
- **Skill** — at [`skills/<name>/SKILL.md`](skills/), seeded to
  `<data-root>/skills/<name>/` by `litany prime`. The frontmatter `description` is
  the tool's description in `tools: [...]`; the body explains when to
  reach for it.

The pool is discoverable from the CLI itself — `litany tool --help`
names it, and a name that is not in it is declined non-zero naming it
too, the same way `load_skill` declines an unknown skill (ARCH §3.3):

```
$ litany tool --help
Arguments:
  <NAME>  Built-in tool to run; one of: apply_patch, bash, cd, dispatch, load_skill, message, read_file

$ echo '{}' | litany tool nosuchtool
litany tool nosuchtool: unknown built-in tool: "nosuchtool"; available: apply_patch, bash, cd, dispatch, load_skill, message, read_file
```

**A direct run gives you the triple, not the envelope.** `litany tool
<name>` *is* the tool binary, so it hands back exactly what the bullet
above says a binary produces: stdout on stdout, stderr on stderr, the
status as the process exit code. The §3.3 *result envelope* — exit code
on the first line, then stdout, then stderr under `--- stderr ---` — is
the **harness's** rendering of those three into one `tool_result.content`,
and nothing at the CLI prints it:

```
$ echo '{"command":"echo out; echo err >&2; exit 3"}' | litany tool bash
out
err
$ echo $?
3
```

Four of the built-ins are not runnable standalone at all. `cd`,
`dispatch`, `message` and `load_skill` read the calling agent's identity
from the harness-set `LITANY_CONV_REPO` / `LITANY_CONV_BRANCH` (ARCH
§3.3), which only a real step supplies, so by hand they decline:

```
$ echo '{"path":"/tmp"}' | litany tool cd
litany tool cd: missing env var "LITANY_CONV_REPO" (set by the harness per ARCH §3.3)
```

Built-ins:

- **`apply_patch`** — the structured edit path (ARCH §3.3 *The patch
  tool*): one patch envelope (codex's `apply_patch` grammar, `*** Begin
  Patch` … `*** End Patch`) carrying add/delete/update/rename across
  multiple files, applied **atomically** — every operation is validated
  and every post-state computed in memory before any write lands, so a
  patch that cannot apply in full applies not at all. Hunks locate
  their context by the four-rung **matching ladder** (exact →
  ignore-trailing-whitespace → ignore-edge-whitespace →
  unicode-normalized, mirroring `git apply`'s fuzz) and the target must
  be unique at the winning rung: ambiguity and stale context are loud
  typed declines naming file, hunk, and reason — never a guessed edit;
  `@@ <enclosing symbol>` anchor lines disambiguate repeated blocks.
  Success returns a JSON report with each hunk's winning rung, landing
  line, and (under fuzz) the lines actually replaced. Try it directly:
  `echo '{"input":"*** Begin Patch\n*** Add File: hi.txt\n+hello\n*** End Patch"}' | litany tool apply_patch`.
- **`read_file`** — read the entire contents of a file at a given
  path. Rejects files larger than 1 MiB, reporting the file's **true**
  size (`stat`, not the capped read's length) so the agent can judge
  the magnitude it is up against; v0.4+ adds the oversized-output
  auto-dispatch shim (ARCH §3.3 / §12). Try it directly:
  `echo '{"path":"README.md"}' | litany tool read_file`.
- **`bash`** — runs a shell command via `sh -c` and hands back the
  shell's own three: its stdout, its stderr, and its exit status
  (`128 + signo` when a signal killed it). The harness renders those
  into the §3.3 *result envelope* the model reads — the exit code stated
  on the first line, then stdout, then stderr under a `--- stderr ---`
  marker whenever the command wrote any, on success as well as failure,
  so a warning from a command that exited 0 is not lost (bl-ffc5).
  The shell runs in its own process group so a SIGTERM the
  harness sends is forwarded to the entire spawned tree (§2.9
  cascade). Its model-facing definition — `skills/bash/SKILL.md`
  frontmatter and `schemas/tools/bash.json` — is deliberately explicit
  that the shell is **local, non-interactive, rooted in the agent's
  current working directory, and stateless between tool calls**: a
  gpt-5.x agent read the older wording as a remote interactive shell and
  told the user it could only see "the server's IP" (bl-298c). A `cd`
  inside the command moves only that one shell — to move for more than
  one call, use `cd` below. Try it directly:
  `echo '{"command":"ls"}' | litany tool bash`.
- **`cd`** — moves the calling agent's working directory for every
  later tool call (ARCH §3.3 *Working directory*). Input is `{path}`; a
  relative path resolves against where the agent currently is, `..` and
  symlinks resolve, and the result is `{"cwd":"<absolute path>"}`. A path
  that names nothing or names a file is declined and the agent stays put.
  The cwd is **one mutable per-agent fact**, stored as the agent's own
  mark `refs/litany/cwd/<agent-id>` — the same per-agent mark namespace
  as `conflicted` / `budget-exhausted`, so it is reaped with the agent by
  `litany delete` and crosses no fork. The default is the agent's
  worktree, so an agent that never calls `cd` behaves exactly as before.
  Nothing is fenced off — `bash` could already reach outside with an
  absolute path — but the tool commit stages only the worktree, so writes
  made elsewhere are real and **uncommitted**: off the branch, invisible
  to a parent, absent from replay. It has no standalone run — moving an
  agent needs an agent, so by hand it declines for the missing
  `LITANY_CONV_REPO` (above).
- **`dispatch`** (v0.4 Phase 2) — spawns a subagent on a fresh
  branch with the supplied goal and returns
  `{"status":"in_progress","handle":"<sub-branch>"}` synchronously
  (ARCH §2.5). Input is `{role, goal}` plus an optional `name` (the
  child's display name, ARCH §2.3);
  the role must resolve to `souls/<role>.md` and a `roles:` entry in
  `providers.yaml` — both read from the calling branch's governing
  config commit (§2.2). Reads the calling
  conversation's repo + branch from the harness-set
  `LITANY_CONV_REPO` / `LITANY_CONV_BRANCH` env vars (ARCH §3.3 env
  bullet); spawns through `litany dispatch <role>` (§3.4). The handle
  it returns is the child's *address* — there is no polling tool to
  pair with it. The substrate redesign (ARCH §2.5 "Dispatch returns
  the child's address") dissolved the handle/`await` pair: the child's
  result comes back as a **deposit into this agent's inbox** carrying
  an epitaph (§2.6, §2.11 — the dispatch is the child's first prompt, so
  the reply comes back here), so `await`/`check` had nothing left to
  observe and are gone. The return path — the result-message deposit
  and the delivery-time work-product transfer — is built and live
  (bl-4ce8, bl-9f53, bl-c33b, §2.6), and **children run full step
  loops**: the dispatch's own front-door deposit finds the fresh child
  quiescent and launches the ordinary driver, `litany advance` (§6) —
  there is no child-specific loop and no worker path — which steps the
  child to a terminal event, deposits its epitaph result (final-response,
  budget-exhausted, or stop) at the address §2.6 names, and revives that
  recipient, which delivers the result at its next step boundary.
- **`message`** — deposits content into an *existing* agent's inbox
  (ARCH §2.11). Input is `{agent, content}`; the recipient is addressed
  by its agent id (its branch name / hyphenated descent) or by the
  unique display name it was dispatched with (ARCH §2.3). Unlike
  `dispatch` it starts no branch and returns no address — it deposits
  synchronously and returns `{"status":"deposited"}`. The sender is the
  calling agent's id, taken from the harness-set `LITANY_CONV_BRANCH`
  (never model-supplied), so provenance cannot be forged. It goes
  through the front door — `litany message` (below) — like `dispatch`
  goes through `litany dispatch`, so it inherits the front door's
  recipient guards: an id that is not a single path component, or one
  with no `agents/*` ref, comes back as an `is_error` result naming the
  decline instead of a silently lost message. **Shipped state:** the deposit lands
  and the step-boundary drain delivers it (bl-1129) — the next driver to
  step the branch moves the inbox file into `messages/` as a transcript
  entry at its next boundary. A deposit into a *quiescent* agent is
  self-delivering: the free-lease probe detach-spawns `litany advance`
  (§6, below), which acquires the lease, delivers the deposit, and steps
  the branch.
- **`load_skill`** — copies a pooled skill's body into the calling
  agent's worktree at `skills/<name>/`, where the next context assembly
  composes it (ARCH §3.3 *Body-on-demand*, §5.2). Input is `{name}`; the
  data-root pool + target worktree come from `LITANY_HOME`/XDG and
  `LITANY_CONV_REPO` / `LITANY_CONV_BRANCH`. Returns
  `{"status":"loaded","path":"skills/<name>"}` on a fresh copy or
  `already_loaded` when the worktree already holds it (the loaded copy is
  the snapshot the branch is pinned to; `rm` and reload to refresh). An
  unknown or non-single-component name is declined (`is_error`, naming
  the available pool). **Shipped state:** the copy commits with the tool
  result — a tool commit now stages the whole worktree (`git add -A`,
  `commit_tool`), landing any tool's worktree side effects with its
  result entry (ARCH §2.3).
- **`multi_tool`** — fans one model round trip into N tool executions
  (ARCH §3.3 *The multi-tool*). Input is `{invocations: [{name,
  input}, ...], on_failure}`: the same `{name, input}` shapes the
  individual tools declare, run **serially in list order** — a later
  entry sees every earlier entry's side effects — with `on_failure:
  "abort"` (default: a failed or declined entry skips the rest) or
  `"run_all"`. All results return together in the envelope's single
  `tool_result`, attributed per entry (`[k/N] <name>:
  ok|failed|declined|skipped`); incremental delivery has no wire home
  (one result per `tool_use` id on every protocol), and nesting is
  declined at depth 1. Each inner invocation passes the same grant gate
  and executor as a top-level one and lands its own diagnostic record
  under a derived id (`<outer-id>-<k>`). The one built-in with no
  `litany tool` subcommand: its binary *is* the step loop
  (`src/prompt/dispatch/tool_step/multi.rs`), so it does not appear in
  the `litany tool --help` pool above, while its schema/skill pair
  installs and grants like any other tool's.

## Naming an agent

An agent may be dispatched with a **display name** — `litany prompt
--name`, `litany dispatch --name`, or the `dispatch` tool's optional
`name` input (ARCH §2.1, §2.3). It is what you say out loud;
the **id** stays the identifier (branch name, worktree directory,
`steps/` and `inbox/` keys) and never carries display semantics.

```
litany prompt <ws> 'survey the crate' --name pale-otter
litany message <ws> pale-otter 'check the Makefile too'
```

- **One home, no registry.** The name is a `name` file committed on the
  agent's own dispatch commit, beside `goal.md`. Reading it is `git show
  agents/<id>:name`, so the `agents/*` refs stay the workspace's only
  registry, worktree teardown cannot lose it, and `litany delete`
  recycles the name with no cleanup step at all — the ref goes, the blob
  goes, the name is free. There is no index file anywhere.
- **Every dispatch commit writes the file; empty means unnamed.** A fork
  inherits its fork point's tree, so the commit overwrites the name
  rather than deleting it — that is what keeps a child's dispatch from
  riding the work-product transfer (§2.6) back into its parent and
  unnaming it.
- **Set once, unique, never id-shaped.** A name is fixed at creation
  like the goal. Creation refuses one a living agent already wears
  (naming the holder), one that is not a single whitespace-free word,
  and one that begins with an agent-id timestamp (`YYYYMMDDTHHMMSSZ`) —
  which keeps names and ids disjoint, so `litany message` never has to
  guess which you meant. Every refusal happens before the fork, so it
  leaves no branch behind.
- **Ambiguity is refused, not resolved.** Creation-time uniqueness
  cannot see a fork-back-in off an already-named commit, so two living
  agents *can* end up wearing one name; addressing that name then fails
  loudly, naming both ids.

## Messaging an existing agent directly

`litany message <workspace> <agent> <content>` deposits a message into
`<agent>`'s inbox and, finding the recipient quiescent, launches a
driver to deliver it (ARCH §2.11, §3.4). The sender is read from
`LITANY_CONV_BRANCH` — the calling agent's id when the `message` tool
re-enters the verb, else `user` for a bare invocation.

- **`<agent>` is an id or a unique name** (ARCH §2.3, §2.11). An exact
  agent-id match wins; otherwise the needle is matched against the
  display names the workspace's living agents wear (see *Naming an
  agent* below), and a unique wearer resolves. A name worn by two
  living agents is **refused with the candidate ids** — never guessed —
  and a needle nothing answers to falls through to the ordinary
  existence decline. This is the only verb that reads names: every other
  id-taking verb addresses an id the harness itself printed.

- **The recipient is guarded before anything is written.** The id must
  be a single path component (ARCH §2.3) — `..`, a `/`, or an absolute
  path is declined, never sanitized, because `Path::join` would honour
  it and write outside the workspace — and an `agents/<id>` ref must
  exist for it: a message is addressed to an *existing* agent (§2.11),
  so a deposit no drain would ever come for is refused (`litany
  message: no agent "…" …`, exit 1) rather than left in an inbox
  directory nothing will ever read. The id guard is the same rule at every verb taking an agent id
  from outside — `message`, `advance`, `stop`, `dispatch`, `bundle` —
  and literally the same code: one workspace-layout guard and one
  existence guard, each carrying the calling verb's own clause for *why*
  it needed an agent, so what differs between verbs is the cause, never
  the phrasing or the remedy.
- The deposit is a create-only file at `<workspace>/inbox/<agent>/
  <sender>-<NNN>.md` (temp-path + atomic rename), with `from:` /
  `deposited_at:` frontmatter and the content as its body. `<NNN>` is
  the sender's own sequence, derived as max-present-plus-one over its
  existing files in that inbox.
- After depositing, the verb probes the **executor lock** (`flock` on
  the inbox directory): the same lease the shipped `litany prompt` step
  loop holds for its whole run, releasing it on exit. A held lease means
  a driver is already stepping the branch (it will deliver at its next
  boundary); a free lease means the branch is quiescent.
- On a free lease the verb launches a driver — `litany advance
  <workspace> <agent>` (ARCH §6) — as a **detached spawn** (§2.11):
  `setsid` (its own session and process group), stdio to null,
  fire-and-forget. The driver outlives the `litany message` process, so
  messaging is scriptable: the verb returns as soon as the deposit and
  spawn land, and delivery + stepping continue in the driver.
- **A failed branch is named, never refused.** If the quiescent
  recipient's latest model call failed (its last `response.json` segment
  terminated in an `error` — retries exhausted or a non-retryable error,
  ARCH §2.10), the deposit and launch proceed unchanged — messaging is
  exactly how such a branch is retried once the cause is fixed — but the
  verb prints a stderr advisory naming the branch and pointing at
  `steps/<agent>/` and `litany scan`, so a silent death (ARCH §2.3, §8)
  is distinguishable from ordinary idleness at the verb that touches
  it. Exit
  code and stdout are untouched.

## Driving a branch: `litany advance`

`litany advance <workspace> <agent>` is the §6 driver verb — the
process every launch seam spawns, and the same verb an operator runs by
hand. One invocation is one **hop**: guard the id (a single path
component, and an `agents/<id>` ref must exist — a name that is no
agent is refused with `no agent "…"` and exit 1 before any lease, so
an operator typo neither drives anything nor leaves an `inbox/<id>/`
behind), take the lease (adopt the
`LITANY_LOCK_FD` fd published by a predecessor hop, else try-acquire
the executor lock — losing it is a clean no-op), deliver pending inbox
messages through the real drain (rematerializing a torn-down worktree
first), derive warrant from the transcript tail (ends user-side → a
model call is due; ends assistant-side without `tool_use`, or empty →
exit silently; assistant `tool_use` with uncommitted results → decline
loudly, the one non-replayable state — unless a hold mark parks it,
see *The tool-control seam* below), run one step, and hand off: a
step that emitted `tool_use` runs its tools and **exec's the successor
`litany advance`** with the lock fd deliberately inherited (close-on-
exec cleared just before exec; the successor fstat-validates the fd
against the inbox directory and restores close-on-exec), while a
terminal event ends the chain through the §2.11 exit protocol. Because
the successor is `exec`'d in the same process, the pid, process group,
and flock lease all survive the hop — `litany stop` lands on whichever
hop is current, and no rival driver can wedge between hops.

### The tool-control seam

An optional `tool_control:` block in the governing `workflow.yaml`
names an adjudicator binary the tool window consults **before every
granted tool invocation executes** (ARCH §3.3 *Tool control*, §6):

```yaml
tool_control:
  command: /path/to/control
```

The control gets the `tool_use` block plus the calling role and agent
id as JSON on stdin and answers one JSON verdict on stdout — `pass`
(the tool runs unchanged), `refuse` (it never runs; the reason reaches
the model as an in-band error result), or `hold` (the invocation parks
before execution for out-of-band review: a `refs/litany/held/<agent>`
mark records what parked and why, the branch exits without a terminal,
and its mail queues). Release is re-adjudication: the next
`litany advance` of the agent consults the control freshly — skipping
already-committed results — so whatever fact lifts the hold (an
approval file, a verifier's verdict) is the control's own contract. A
control that cannot answer **fails closed**: the invocation does not
run and the step aborts loudly. No control ships — omit the block and
no control is consulted; the seam is the shipped surface.

## The exit protocol and the operator scan

Normal operation needs zero scanning (ARCH §2.11): `litany message`
deposits, probes the executor lock, and launches a driver if the agent
is quiescent; the executor drains its inbox at every step boundary. The
graceful-exit crack — a deposit landing after an executor's final drain
but before its lock release — is closed by the **exit protocol**
(§2.11, bl-5846): one terminal sequence, no agent kinds — deposit the
result message (a structural no-op for a parentless agent) → release
own lock → spawn a driver at own agent, fire-and-forget → probe-and-
launch at the parent the deposit just landed in → exit. Two
pins terminate the recursion: a driver that acquires and finds nothing
to deliver exits silently (no step, no epitaph, no further launch —
`dispatch::driver::drive` is that entry), and the launch is decided by
epitaph value — a final response launches; `stopped` and
`budget-exhausted` never do. The exit launch rides the same launcher
seam as the writer probe, so it is the same detached `litany advance`
spawn (§6); the decision logic, ordering, driver entry, and the spawn
itself are live and tested.

The parent-side step is what makes **revival-on-deposit** real
(bl-4a6c): a child that returns to a quiescent — even torn-down —
parent starts that parent's driver itself, through the *same*
`probe_and_launch` the `litany message` verb uses (one probe, no
second copy), so the parent rematerializes, delivers the result, and
steps with no `litany scan` in the path. A parent whose lease is held
gets nothing launched: its running executor delivers at its next step
boundary. The epitaph decision governs this launch too, one level up:
a `stopped` child would otherwise wake its parent to react to — perhaps
re-dispatch around — the very branch the operator killed, and a
`budget-exhausted` child's ceiling is the whole tree's (§6), so the
woken parent would exhaust on its own next check and deposit again. In
both cases the result still lands in the inbox and waits for the next
explicit touch.

Crashes are accepted as a failure class (§2.11): everything is on disk,
so a hard death strands results and messages *late*, never lost, and
the next touch heals. That touch is a user reprompt — or the operator
verb **`litany scan <workspace>`** (§2.11, §8, bl-d148 + bl-5846): one
workspace-wide pass, run by hand or by cron if you want a heartbeat,
never wired into any driver hot path or default schedule (the events it
compensates for happen at crash rate, not step rate). Two derived
actions, no watcher (an idle workspace stays unswept until the next
touch, by design):

- **Silent-death sweep.** Every agent branch with no live executor (the
  §2.11 executor-lock probe) that either died mid-work — its latest
  step's model call never settled complete: `response.json` closed
  without a terminal `end` (killed/stopped, §2.9), *or* its final
  segment terminated in an `error` (retries exhausted or a non-retryable
  error, §2.10 — that segment closes with a clean `end`, so
  absence-of-`end` alone would misread the branch as idle) — or, for a
  child, never deposited a result message is a *silent death* (the §8
  health count). Each one is **named** in the report
  (`silent deaths: 1 (<agent-id>)`): a dead **root** gets no deposit —
  it has no parent inbox — so its name here is how an operator learns
  which branch went quiet, and `steps/<agent-id>/` is where to read
  why. For each hard-crashed **child** in that set, the sweep deposits
  a `died`-epitaph result message *on the child's behalf* (sender = the
  child — the sweep is the scribe, not the author), so the parent is
  revived rather than stalled. The "never deposited" test reads both the
  parent's inbox (undelivered) and its transcript (delivered), so a prior
  sweep's own deposit is seen on re-scan and never re-deposited —
  idempotent by construction.
- **Inbox flush.** Every agent with pending inbox files and a free lock
  gets a driver **launched** — never drained: the scanner moves no files
  and commits nothing; only an agent's own lock-holding executor
  delivers. An agent whose lock is held is left alone. The inbox listing
  is intersected with the `agents/*` refs — the one registry of who
  exists — so an inbox directory with no matching ref is reported
  (`inboxes with no agent branch: N`) and left in place rather than
  driven: a driver launched for a name with no branch is refused by the
  existence guard (`litany advance: no agent "…"`, exit 1) on this pass
  and every pass after, writing nothing. The sweep's own deposits
  are picked up by the flush that follows in the same pass.

**Shipped state.** The scan (silent-death sweep + inbox flush) ships
behind `litany scan` and *only* there — driver startup (`litany prompt`,
`litany dispatch`, `litany advance`) runs no workspace scan. The flush
and the exit launch reuse the same driver-launch seam as `litany
message`, and the spawn is real: each seam decides *when* a driver is
needed and detach-spawns `litany advance` (§6) for it. Children run full
step loops (bl-c33b), so a `died` child is a state a real run reaches; the
derivation is additionally exercised against constructed on-disk states,
since a hard crash is not reproducible on demand.

**Namespace note.** The candidate enumeration is the `agents/*` ref
namespace, exactly as ARCH §8 writes it (a root is `agents/<conv-id>`,
a child `agents/<parent>-<sub-id>`); config branches are excluded
structurally by the prefix — there is no `main` (§2.2).

## Dispatching subagents directly

`litany dispatch <role> <repo> <branch> [--goal <text>] [--from <ref>]
[--name <name>] [--pin <dest>=<src>]... [--cwd <path>]` is the §3.4 re-entry point
every child dispatch uses.
It is **writer-shaped, not an
executor** (ARCH §2.1): it forks the child branch, lands the dispatch
commit, and deposits the dispatch message through the same front door
every sender uses — the driver that deposit launches is the ordinary
`litany advance` (§6). The role name is positional and the role set is
**open** (§4.3): a role is dispatchable iff the calling branch's
governing config commit lists it under `providers.yaml` `roles:` and
carries `souls/<role>.md`. The CLI enumerates no role names, so a
verifier, a critic, or a role you author needs no CLI change; validity
is checked *before* the fork, so a rejected role leaves no branch debris.

The **id guard runs first**, through the same two functions `message`,
`advance`, `stop` and `bundle` call: the workspace layout, then the
dispatching parent's `agents/<id>` ref. So all three refusals are the
product's, never git's:

```
litany dispatch worker <no-such-ws> someagent --goal hi
  → <path> is not a workspace (no repo.git) — create one with `litany new` (ARCH §2.2)
litany dispatch worker <ws> nosuchparent --goal hi
  → no agent "nosuchparent" in this workspace — a child forks off an existing parent (ARCH §2.5); …
litany dispatch verifier <ws> <agent> --goal hi
  → role "verifier" is not defined in the providers.yaml that will govern a child of agent "<agent>" — defined roles: compactor, worker
```

The role refusal names the pool that *is* defined — the same "name the
pool" idiom `load_skill` and `litany tool` decline with — and names the
control file the user knows rather than the config commit's sha.

`--from <ref>` forks the child off that ref instead of the parent's tip
— the ordinary fork with a ref argument (ARCH §2.3, §7.2), which is what
the §6 verifier gate already does when it forks a judge off the worker's
terminal ref. The child is still `<parent>-<sub>`, so its return address
— where its obituary goes, and where its reply goes until somebody
else prompts it — is still the dispatcher's (§2.6). Its **config follows the fork point**:
control is read from that ref's governing config commit (§2.2 — "an
agent started by fork-back-in inherits its source's config the same
way"), which is the commit every later `litany advance` resolves from
the child's own branch, so the soul, the grant, the descriptors and the
budgets cannot disagree with what the child's steps will read. A fork
point whose lineage does not define the role is declined by name; an
absent ref is declined by the same guard `--from` uses at `prompt`,
ahead of the fork, so neither leaves branch debris.

`--pin <dest>=<src>` is exactly `litany prompt`'s (above, one
mechanism): the child's dispatch commit snapshots the named bytes
beside `goal.md` + `soul.md`, refusals fire before the fork, and the
harness-initiated dispatches (compactor, verifier) pin nothing — the
same path with empty inputs.

- `litany dispatch compactor <workspace> <conv-id>` forks a
  compactor-souled child off that agent's tip — exactly what a due
  compaction checkpoint does (§2.7), run by hand. The compactor is an
  **ordinary child that makes a real model call** through `bz`; it is not
  a stub, and it does not merge anything itself. Its goal is
  procedure-generated, so passing `--goal` is rejected. Its toolset is
  the deletion-only pair injected for the compactor role alone (never a
  `providers.yaml` `tools:` list): `write_summary`, which writes the next
  `summary/<NNN>.md` on the compactor's branch, and `mark_for_deletion`,
  a staged `git rm` that can remove but never write content — so the
  worst case is lost information, never corrupted information. What it
  may never nominate is what is not the branch's history to shed: what
  the dispatch wrote and never rewrote — the branch's **dispatch entry**
  (`messages/001-…`), which is the conversation's opening prompt and so
  the goal in transcript form, and the system slot's `goal.md`,
  `soul.md` and `name`, which every model call on the branch is composed
  from — and **what this pass itself wrote**, which is the summary it is
  producing. An *earlier* pass's summary stays nominable, and
  superseding one is what the compactor is told to do; the class is the
  pass's own output, read off what changed after its own dispatch
  commit. Every such nomination is declined in-band (ARCH §2.7). Its
  request *declares* more than that pair: a compactor inherits the
  dispatching branch's transcript, so the model call also names whatever
  tools that transcript used — otherwise the provider refuses a request
  whose history mentions a tool it was not told about. Declaring is not
  permitting: a compactor reaching for one of those inherited tools gets
  an error tool result naming its own two, and nothing runs. The
  **compaction landing** happens later and elsewhere: when the
  compactor's result message is delivered, the dispatching agent's own
  executor interprets its `compactor_return: land_compaction` binding
  (§6) and lands the product by **rebase-forward** (§2.6) — the
  compaction span squashes into a single compaction base (the summary
  added, the nominated deletions applied, subject
  `compaction base [<compactor-id>]`) and every commit after the
  compaction point replays on top; nothing merges anywhere. A compactor
  that ends on any other epitaph lands nothing; the branch simply
  continues uncompacted — enforced where the binding is interpreted:
  the delivered result's **epitaph value** gates `land_compaction`, and
  a `died`/`stopped`/`budget-exhausted` compactor return is delivered
  like an ordinary child's result instead, so the parent sees the
  epitaph and nothing of the compactor's branch crosses (§2.6, §2.7). A
  replay conflict git cannot resolve on its own is **declined** rather
  than committed: a modify/delete on a work product the live branch
  rewrote resolves live-branch-wins, any marker-writing conflict aborts
  the rebase and marks `refs/litany/conflicted/<compactor-id>`, and a
  pass another landing overtook is superseded and lands nothing — so
  marked-up text can never reach a `summary/**` file that is composed
  into the next model call.
- `litany dispatch worker <workspace> <parent-id> --goal <text>`
  spawns a worker child off the parent's tip. The new id is
  `<parent>-<sub-id>` (hyphenated descent, §2.2), its ref
  `agents/<parent>-<sub-id>` (§2.3), its worktree
  `agents/<parent>-<sub-id>/`; `goal.md` carries the supplied text and
  `soul.md` is read from the governing config commit of the ref the
  child forks off — the parent's own branch unless `--from` named
  another (§2.2)
  (`souls/worker.md`, §2.2), both committed as the dispatch commit
  (§2.3 step 2). The child then **runs a full step loop** under the
  `litany advance` driver its dispatch deposit launched, and at its
  terminal event deposits a result message — epitaph, terminal ref, and
  the terminal response iff it spoke — at the address §2.6 names (the
  dispatcher, unless somebody else has spoken to the child since),
  reviving that recipient if it had gone quiescent (§2.6, §2.11). The v0.4 "Phase 1
  stops at the dispatch commit" worker path (`worker.rs`) is **deleted**,
  not extended (bl-c33b).

## Providers

Every model call goes through **brazen** — one small, stateless binary
(`bz`) that adapts every provider and wire protocol behind a single pipe
contract (see [ARCH §4.4](docs/ARCHITECTURE.md#44-the-provider-adapter-brazen)):

```
stdin (canonical request, JSON) → bz → stdout (v=1 event stream, NDJSON, one terminal `end`)
```

The harness execs `bz --json --provider <row>` once per attempt, pipes a
typed `brazen::CanonicalRequest` on stdin, and appends bz's stdout
verbatim to the step's `response.json`. litany links the `brazen` crate
(`brazen = "=0.0.6"`) for the canonical *types* only — the data plane
always crosses the subprocess boundary (§3.4). Two facts follow:

- **Retry is the harness's.** brazen never retries — one `bz` process,
  one HTTP round-trip. On a retryable in-band `Error`
  (`CanonicalError::retryable()`, the linked crate's single home for the
  fact) the harness re-invokes `bz` up to the `workflow.yaml` attempt cap
  (§2.10). Each attempt appends one segment to `response.json`; the last
  is authoritative. Each attempt's `bz` stderr appends to the step's
  `stderr.log` beside it — empty on an ordinary run, because brazen
  speaks its failures in-band on stdout. A `bz` that dies *before* it can
  (a malformed brazen config) leaves an empty stream that reads exactly
  like a mid-stream kill, so the half-stream error quotes that capture's
  tail; with a stop pending it stays quiet, because the stop check point
  (§2.9) discards the outcome before anything is rendered.
- **Auth and endpoints are brazen's.** Provider *rows* (endpoint,
  protocol, auth mode, model aliases) live in brazen's own config
  (`~/.config/brazen/config.toml`; `bz --dump-config`, `bz --login`).
  litany references a row by name and never sees credential material
  (ARCH §4.1). A load-time guard (`bz --version` == the linked crate
  version) rejects a mismatched binary; `make install` installs the pin
  with `cargo install brazen --version =0.0.6`.
- **A failed model call names the row.** Which row litany routed a model call
  under is litany's fact, not brazen's (it is the role's `provider:` in
  the config commit's `providers.yaml`), so the harness states it:
  `provider error (<kind>) on provider row "<row>": <message>`. A
  missing credential — brazen's `auth` kind, what a 401/403 normalizes
  to — additionally states the fix, with the row substituted in:

  ```
  litany prompt: provider error (auth) on provider row "anthropic": no credential for this
  provider … — no credential is reaching that row; authenticate it with
  `bz --login --provider anthropic`, or export the API-key env var it is configured to read.
  `bz --list-providers` shows every row's auth mode and credential state …
  ```

  On an operator-run `litany prompt` that lands on your terminal; a
  detached driver writes it to `<workspace>/steps/<agent-id>/driver.log`
  (ARCH §2.11).

### Adding a provider

- **A new provider on a supported protocol** is a brazen config row — no
  code anywhere. Add the row (`bz` config), then point a role at it in
  `<repo>/providers.yaml` (`provider:` = the row name, `model:` = the
  wire model id — the whole binding, §4.3).
- **A new wire protocol or auth mode** is a contribution to brazen.
- **An alternate adapter binary** that honors the same pipe contract
  slots in via the optional `adapter:` path in `models.yaml` (§4.2); the
  version guard is skipped for it and the in-band `MessageStart.v`
  handshake governs compatibility instead.

## UI (v0.5)

The desktop frontend lives in its own repository, `yog`: an
egui/eframe window that renders a workspace and issues user actions via
`litany <subcommand>`. It composes on litany's public surfaces only —
the CLI and the on-disk workspace layout (ARCH §3.5, §7.1) — and takes
no Cargo dependency on this crate, so it builds, versions, and installs
independently (`make install` there drops `yog` next to
`litany`). Keeping frontends out of this workspace is deliberate:
litany ships as a composable component, and anything that composes it
(a GUI, a web view) lives outside it and meets it at those surfaces.

## Evaluation: archival and the task suite (§9)

**Archive a run.** A "run" is an agent subtree, not a whole workspace (§9.2).
`litany bundle <workspace> <agent> <out-dir>` writes the subtree — the
`agents/<agent>` branch and its `agents/<agent>-*` hyphen-descendants (§2.3),
with all the ancestry those refs reach — plus the subtree's **governing
lineage**: every `config/*` ref whose history reaches it (§2.2). Both go into
one `git bundle`, and the matching `steps/<id>*` and `inbox/<id>*` diagnostic
slices are copied beside it. One bundle plus two slices is the whole run.

The config refs are not decoration. An agent's control files are read from its
*governing config commit*, which is derived — the nearest ancestor of the
branch reachable from a `config/*` ref (§2.2). Ancestry alone carries that
commit as an object but names no ref to take the merge-base against, so a
replay of the agent refs alone yields a workspace no verb can drive. Carrying
the refs (never a sidecar file — the refs are the single source) makes the
replayed repo derive its governing config by the same computation, over the
same candidate set, as the workspace it came from. "Every ref whose history
reaches it" is broader than "every ancestor": a **sibling** config lineage
that shares only a common root with the bundled subtree is still a
merge-base *candidate*, so it rides too — carrying a ref that turns out not
to be the nearest one is how the bundle stays a faithful copy of the
computation, not a leak.

```
litany bundle /path/to/workspace <agent-id> /path/to/archive
```

**Replay a run.** `litany replay <archive>` reconstructs a scratch workspace
under `LITANY_HOME`'s data root at `replays/<primary-id>/` (the primary id is
the subtree's root agent), fetches every branch out of the bundle into a
fresh bare `repo.git`, materializes the primary's worktree under `agents/`,
restores the slices, and prints the scratch path. Point the ordinary frontend
at it — replay is not a mode (§2.3). Set `LITANY_HOME` to an isolated
directory to keep the replay sandboxed; the harness root it points at still
supplies the machine-local pieces a config only *names* (`models.yaml` and the
brazen provider rows, §4.2/§4.4).

A replayed workspace is an ordinary workspace: `litany prompt <scratch> "…"`
forks a fresh root off the config head that rode the bundle, and `litany
message` / `litany advance` drive the replayed agent on its own governing
config commit.

```
LITANY_HOME=/tmp/replay litany replay /path/to/archive
```

**Delete a run.** `litany delete <workspace> <agent> [--children] [--dry-run]`
removes an agent and every slice of it (§9.2 *Retention and GC*): the
`agents/<id>` ref, the worktree under `agents/<id>/`, the `steps/<id>/` and
`inbox/<id>/` directories, and every `refs/litany/<kind>/<id>` mark. `bundle`
composes in front of it — **bundle-then-delete is the archive path**, delete
outright is the other, and neither verb carries a flag for the other.

```
litany delete /path/to/workspace <agent-id> --children --dry-run   # the plan
litany delete /path/to/workspace <agent-id> --children             # the act
```

Two refusals, both checked across the whole subtree before anything is
removed:

- **A subtree is never implied.** Bare, an agent with `<id>-*`
  hyphen-descendants (§2.3) is declined, naming them; `--children` is the
  explicit request for the whole subtree (the shape of `stop --stop-children`,
  §2.9).
- **A live driver is never reaped.** An agent whose executor holds the §2.11
  lock is declined, naming the lock; `litany stop` it first and delete once it
  is quiescent.

The verb's one product is the census of what dies, identical in both moods —
so a frontend's confirmation dialog enumerates exactly what the receipt will
later confirm:

```
would delete 20260101-p1; descendants: 1 (20260101-p1-20260102-c1); pending deposits: 2
deleted 20260101-p1; descendants: 1 (20260101-p1-20260102-c1); pending deposits: 2
```

`pending deposits` counts undelivered mail addressed **to** the subtree, which
dies with its inboxes; a message one of these agents *sent* already lives in
the recipient's inbox and survives.

**Re-running a delete is how a half-finished one finishes.** The target set is
the union of the id's five homes rather than the ref list, so a delete
interrupted anywhere leaves a state the next run completes, and a delete of an
agent nothing remembers is a quiet success with an empty census — no
partial-delete limbo, and no `--force` to reach for. Deletion is an operator
act on the operator's own schedule: nothing expires on a timer, and the
harness ships no default retention window.

**Task suite.** The evaluation suite lives as data under `tests/suite/` — 50
tasks with machine-checkable `check` scripts, tagged by the seven §9.1 failure
categories (≥10 per category), format in `tests/suite/README.md`,
well-formedness enforced by `tests/suite.rs`.

**Run the suite.** The `agent-eval` runner (a separate crate, `crates/agent-eval`,
ARCH §9.3) executes an experiment against the suite N times per task and reports
quality — pass@1 (with 95% Wilson intervals) and pass@5, overall and per
category — plus efficiency (outer wall time per run; and, for runs whose driver
reported a workspace, model attempts, tool invocations, and the four canonical
usage counters) and the run's reproducibility inputs (bl-36fa):

```
agent-eval run --config baseline --suite tests/suite --runs 5 --agent litany-eval-agent
```

`--record <out.json>` also saves the machine-readable evaluation record, and

```
agent-eval compare baseline.json candidate.json
```

renders per-task, per-category, and total baseline → candidate deltas from two
saved records — quality and efficiency side by side, each record carrying its
own reproducibility inputs (suite revision, starting fixture identity,
experiment, driver command + version, observed models/providers, run count).
`compare` runs nothing: no driver, no model. A metric one side never reported
is `—`, never a fabricated zero, and no price is ever inferred — litany has no
tokenizer and reports only provider-reported counters.

`--config <name>` names an experiment — a `workflow.yaml` variant under
`experiments/<name>/` (a config diff, no code changes; see `experiments/README.md`).
`baseline` is the shipped default itself: its `workflow.yaml` is a symlink to
`template/workflow.yaml`, because an experiment is a diff against the default and
the baseline's diff is empty.
Per run the runner seeds a fresh isolated `LITANY_HOME` and working directory,
runs the task `setup`, invokes the agent, then runs the task `check` — **exit 0
is the sole pass signal** (§9.1), so success is observable state, never the
agent's own claim. `--bundle-dir <dir>` archives failing runs for triage via
`litany bundle` (§9.2). The runner is fully tested against a faked agent, so it
needs no live model to validate.

**The shipped driver is `litany-eval-agent`** (`crates/litany-eval-agent`,
workspace-internal like the runner; installed on `PATH` by `make install`).
`--agent <cmd>` stays required with no default: which driver runs the agent
under test is an experiment-defining input, so it is named explicitly. Per run
the shipped driver seeds the run's isolated `LITANY_HOME` from the machine's
litany config root (`models.yaml` plus the `template/` config-root override —
the wire is machine-local by design, §4.2/§9.2, and those two front doors are
how a machine points evaluation runs at its own provider rows), then drives
the harness exclusively through the front door, exec'ing `litany` from `PATH`:
`litany new`, `litany config` (applying the experiment — below), and one
`litany prompt` carrying the task prompt grounded in the shared working
directory. The contract any driver must honour, per run:

| Given | How |
|---|---|
| the task prompt | argv[1] |
| the isolated harness root for this run | `LITANY_HOME` in the env |
| the experiment's `workflow.yaml` | `LITANY_EXPERIMENT` in the env — an absolute path |
| where to report back | `LITANY_EVAL_REPORT` in the env — a file path |
| the working directory | cwd (shared with the task's `setup` and `check`) |

One non-run invocation exists beside the contract (bl-36fa): the runner probes
`<driver> --version` (as argv[1], with none of the run env) once per
evaluation and records the first stdout line among the reproducibility
inputs. A driver should answer with one identifying line and exit; one that
fails or prints nothing is recorded as `version unreported`, never guessed at.

`LITANY_EXPERIMENT` is a hand-off, not a hook: **nothing in the harness reads
that variable.** The harness takes its `workflow.yaml` from the workspace's
config commit (§2.2), never from the environment, so *applying* the experiment
is the driver's job. The shipped driver does it through `litany config`, with
`$EDITOR` set to copy the experiment over the authoring checkout's
`workflow.yaml` — the experiment lands as an ordinary config commit, exactly
the "config diff, no code changes" §9.3 promises (for `baseline` the diff is
empty and the authoring pass declines: the default is already in force).

`LITANY_EVAL_REPORT` names a file the driver **may** write with exactly two
lines — the workspace path, then the agent id — which is what `litany bundle`
needs to archive the run if it fails (§9.2). It is the driver's only channel
back to the runner, and it is also where the run's efficiency metrics come
from (bl-36fa): a disclosed workspace lets the runner read attempts, tool
invocations, usage counters, and observed models off its `steps/` slice.
Writing nothing, or anything malformed, only makes a failing
run un-bundleable and its metrics unreported (`—`, distinct from 0); it is
never an error, and it never affects pass/fail, which
is the task `check` alone. The driver's own exit code is likewise ignored.
Failure to *spawn* the driver, by contrast, is a hard error naming the program.

## Fleet demo

The fleet demo now lives at `~/ops/fleet` — it was a consumer artifact, not
part of the binary, and did not belong riding in the harness repo. It showed
that litany hosts a five-role agent fleet (coordinator, shepherd, sensor,
builder, steward) entirely as configuration, with no harness change. The five
harness defects it surfaced (bl-475a, bl-4231, bl-5a1f, bl-a900, bl-e3f5) are
fixed and pinned as in-repo regression tests.

## Contributing

The instructions below are for contributors building litany from source.
Users installing a release don't need any of this — **[Install](#install)**
covers the three user-facing routes, only one of which involves a clone.

### Contributor setup

```
make install-hooks
```

Sets `core.hooksPath` to `.githooks`. Required on every fresh clone — git
does not track `.git/config`, so the hooks are not active until installed.
That arms both the [pre-commit gate](#pre-commit-hook) and the
[auto-push hook](#auto-push-hook).

The Rust toolchain is pinned in `rust-toolchain.toml` (channel `1.95.0`, with
`rustfmt`, `clippy`, and `llvm-tools-preview`). rustup reads it automatically
for every `cargo` command in the tree and installs the pinned toolchain on
first use — no manual `rustup` step. This is what keeps `fmt-check` and
`lint` from drifting between your machine, another agent's, and CI.

### Build targets

| Target                | What it does                                          |
|-----------------------|-------------------------------------------------------|
| `make build`          | `cargo build`                                         |
| `make release`        | `cargo build --release`                               |
| `make test`           | `cargo test`, with the pinned `bz` first on `PATH` (below) |
| `make test-install`   | `cargo test --test install` — the install contract end-to-end, uninstrumented (it is `cfg_attr(tarpaulin, ignore)`, so `coverage` skips it); ~45s warm, and it re-installs `bz` at the `brazen` pin |
| `make coverage`       | `cargo tarpaulin --fail-under 100` (llvm engine), same pinned `PATH` (below); hard-gated on tarpaulin **0.35.2** exactly (`TARPAULIN_PIN` in the `Makefile` — its one home; any other version aborts with the `cargo install cargo-tarpaulin --version 0.35.2 --locked` fix-it line) |
| `make lint`           | `cargo clippy --all-targets -- -D warnings`           |
| `make fmt`            | `cargo fmt`                                           |
| `make fmt-check`      | `cargo fmt --check`                                   |
| `make schemas`        | Regenerate `schemas/*.json` from the Rust types       |
| `make new-workspace DEST=<path>` | Create a workspace (bare repo.git + first config commit from `template/`) |
| `make eval CONFIG=<exp> SUITE=<dir> RUNS=<n> AGENT=<driver-cmd> [RECORD=<out.json>]` | Run the evaluation runner (ARCH §9.3): experiment × suite × N (see **Task suite** above). `AGENT` is required and has no default — the shipped driver is `litany-eval-agent` (see "Run the suite"), and naming it is deliberate: the driver is an experiment-defining input. `RECORD` saves the evaluation record `agent-eval compare` consumes (bl-36fa). Always an explicit operator command — a live-model eval names its run count and spends money, so it is never CI |
| `make check`          | `fmt-check` + `lint` + `coverage` + `test-install`    |
| `make ci`             | Alias for `check`                                     |
| `make smoke`          | Live-wire smoke test: one real `litany prompt` against the shipped defaults (override with `SMOKE_PROVIDER`/`SMOKE_MODEL`); the default needs a `bz` anthropic credential and spends money; NOT part of `check` |
| `make install-hooks`  | Point git at `.githooks/`                             |
| `make install-bz`     | Install the provider adapter `bz` on your `PATH` at the version Cargo.toml pins (ARCH §4.4); a no-op when the `bz` there already matches. For *running* litany — the tests feed themselves (below) |
| `make brazen-pin`     | Print that pinned version and nothing else — CI keys its `bz` cache on it so no workflow file names a version |
| `make install` [`INSTALL_PREFIX=<p>` `LITANY_HOME=<h>`] | Release-build; drop `litany`/`agent-eval` into `$INSTALL_PREFIX/bin` (default: `~/.local/bin`); install the provider adapter `bz` via `make install-bz` at the version Cargo.toml pins (the ARCH §4.4 version pin — the number's one home); then invoke `litany prime` to found the harness root — config root (default `~/.config/litany`) with a default `models.yaml` and an empty `workflows/` templates dir, data root (default `~/.local/share/litany`) with the `tools/`/`skills/` pools and the `workspaces/` tree — seed-if-absent (ARCH §2.2); `LITANY_HOME` collapses both |
| `make uninstall` [`INSTALL_PREFIX=<p>` `LITANY_HOME=<h>`] | Remove the installed binaries; leaves the harness homes (config + data roots) in place |
| `make image` [`CONTAINER_ENGINE=docker`] | Build the OCI image from `Containerfile`, tagged `litany:<Cargo.toml version>` and `litany:latest`, then run `image-scan` on it. Pushes nothing (see "As a container image") |
| `make image-scan` | The image-side disclosure gate: the planted-secret self-test, then the built image's authored layers and config against `scripts/leak-rules.sh`. A step of `image`; run it alone to re-judge an image already built |

### The pinned adapter under test

The e2e tests exec the **real** `bz` (against a mock HTTP endpoint, not a
provider), and litany's load-time version guard (ARCH §4.4) demands the
pinned version *exactly*. The pin's one home is the `brazen = "=<version>"`
line in `Cargo.toml`.

**The trap.** `bz` normally resolves from `PATH` — that is
`~/.cargo/bin/bz`, machine-global mutable state shared by every checkout and
every agent on the box. Anyone running `make install` rewrites that binary at
*their* tree's pin. If your tree pins a different version, your next test run
dies in five-plus e2e tests with

```
bz version "0.0.5" does not match the linked brazen crate "0.0.6"
```

which looks nothing like "someone else installed a binary" and everything
like a regression you just wrote.

**The cure.** `make test` and `make coverage` do not use the `PATH` `bz` at
all. They depend on `$XDG_CACHE_HOME/litany/bz/<pin>/bin/bz` — installed
from crates.io on first use — and put that directory *first* on `PATH` for
the run, so the tests always exercise the pin **this** tree names, whatever
the machine's `bz` happens to be. The version comes from `BRAZEN_PIN` in the
`Makefile`, derived from `Cargo.toml`; the cache directory is named after
it, so bumping the pin is a cache miss and nothing else, and a stale entry is
never overwritten in place. Cost: one `cargo install` (~25s) per pin per
machine — sibling worktrees share the cache — and nothing at all when warm,
since it is an ordinary make file prerequisite.

Two consequences worth knowing:

- **Bare `cargo test` is still exposed.** It inherits your `PATH` and so
  runs whatever `bz` is installed there. Use `make test`; if you must run
  `cargo test` directly, `make install-bz` first to line the global binary up
  with the tree's pin.
- **No test writes the global `bz`.** `make install` does — that is its job —
  but the install test that runs it (`tests/install.rs`) points
  `CARGO_INSTALL_ROOT` at a per-worktree root under `target/`, so the pinned
  `bz` lands there and `~/.cargo/bin/bz` is never touched by a test run.
- **Runtime resolution is unchanged.** This is test determinism only —
  `litany` itself still resolves the adapter per ARCH §4.4 (the `models.yaml`
  `adapter:` override, else a binding-injected target, else `bz` on `PATH`),
  and `make install` still puts the pinned `bz` on your `PATH` for real use.

`the_makefile_derives_the_same_pin` (`src/prompt/tests/pin.rs`) keeps the two
readers of that one line honest: the Makefile's `BRAZEN_PIN` (which names the
cached binary) and the crate's `brazen_pin()` (which the version guard
compares against) must agree, or the tests would fail the guard against a
binary the Makefile itself installed.

### Workflow

All changes land on `main` via `bl` squash-merges. Direct commits to `main` are
rejected by the pre-commit hook, and every landing on `main` is pushed to
`origin` automatically (see **[Auto-push hook](#auto-push-hook)**).

```
bl prime --as <you>
bl claim <task-id>              # creates a worktree; cd into it
# ...edit, test, commit...
bl close  <task-id> -m "..."    # squash-merges into main; run from the repo root
```

See `bl skill` for the full guide.

### What gets published

`cargo package` ships the crate, not the repo. `Cargo.toml`'s `exclude` keeps
out everything that serves this git checkout only — `docs/`, `tests/`,
`experiments/`, `scripts/`, `.github/`, `.githooks/`, `.balls/`, `Makefile`,
`tarpaulin.toml`, `release-plz.toml`, `AGENTS.md`, `CLAUDE.md`, and
`rust-toolchain.toml` (which would otherwise force a source builder onto this
repo's exact pinned toolchain). What remains is `src/`, `README.md`, `LICENSE`,
`Cargo.lock`, and the embedded asset trees `template/`, `schemas/`, `skills/`,
`install/models.yaml` — those four are `include_dir!`/`include_str!` inputs, so
excluding any of them is a build failure, not a smaller tarball. Verify a change
to the list with `cargo package --list` and then `cargo package`, which
compiles the extracted tarball.

`crates/agent-eval` is `publish = false`: it is workspace-internal and is not
part of the published crate at all.

**The image is a second publication channel, and the build context is its
`exclude` list.** `Containerfile` `COPY`s by name and `.containerignore` keeps
the rest from being sent at all, so the same question — *what did we ship that
we did not mean to* — is asked once per channel and answered in two different
files. Verify a change to the container side with `make image-scan`, which
reads what the built image actually holds rather than what the `COPY` lines
promise; it is the analogue of running `cargo package --list` before a
release, and unlike that one it is a step of `make image` rather than a habit.

### Pre-commit hook

`.githooks/pre-commit` enforces three rules on every commit:

1. **No direct commits to mainline.** `main` and `master` are rejected unless
   the commit is the tail of a merge (`MERGE_MSG`/`SQUASH_MSG` present), which
   is how `bl close` lands squash-merges.
2. **300-line cap on code files.** The cap is a repo *invariant*, not a
   per-commit property, so the hook sweeps **every tracked code file in the
   tree** (`git ls-files`), not just the staged set — a file that crosses the
   cap in one commit and is untouched afterward is still caught. Docs (`*.md`,
   `*.txt`), config (`*.toml`, `*.yaml`, `*.yml`, `*.json`, `*.lock`),
   `Makefile`, `.gitignore`, `LICENSE`, and anything under `.githooks/` are
   exempt.
3. **`make check`** on every commit that touches a Cargo project: `fmt-check`
   (formatting), `lint` (`clippy -D warnings`), `coverage` (`cargo tarpaulin
   --fail-under 100`), and `test-install` (`cargo test --test install`). The
   hook invokes `make check` rather than re-listing the commands, so the close
   gate is always exactly what `make check` is — the Makefile is the single
   source. Formatting and lint drift therefore cannot land invisibly.
   `test-install` is a separate step because the install test shells out to a
   release build and `cargo install brazen`, which contend with tarpaulin's
   `target/` lock; it is `cfg_attr(tarpaulin, ignore)`, so without its own
   uninstrumented step the install contract — the first thing every user
   touches — would never run at the gate at all. It costs ~45s warm and leaves
   the machine-global `~/.cargo/bin/bz` alone: the test redirects `make
   install`'s `cargo install brazen` into a per-worktree root under `target/`
   with `CARGO_INSTALL_ROOT`, so a sibling worktree at another pin is never
   rolled over. The
   toolchain is pinned in `rust-toolchain.toml` and the tarpaulin version in
   `tarpaulin.toml` (also
   `.github/workflows/ci.yml`) so `fmt-check`, `lint`, and the coverage
   denominator mean the same thing locally and on CI — newer tarpaulin
   releases have silently dropped inline `#[cfg(test)] mod tests;` files from
   the count, weakening the floor. `make coverage` aborts with an install
   hint if the local tarpaulin version drifts.

A floor of exactly 100% only holds if every line's coverage is caused by the
code's own structure and not by winning a race, so **no line may be reachable
only while a clock has not yet run out.** With several agents measuring
coverage at once, whichever side of such a race the machine happens to pick
that minute decides the verdict, and the gate reports an uncovered line on a
diff that touched nothing. Two shapes to write around:

- *A retry budget is a count of attempts, never a wall-clock deadline.*
  `PROBE_RETRIES` (`src/prompt/tests/exit_launch.rs`) is the one budget every
  executor-lock probe shares; a deadline expires on load rather than on
  evidence, so under load the give-up arm can be taken on the first pass and
  the retry arm never runs at all.
- *A poll loop waits because its child is still running, not because a flag
  has yet to land.* `wait_with_cascade` (`.../builtin/bash/mod.rs`) and
  `wait_with_stop` (`.../tool/subprocess.rs`) therefore sleep between the
  reap and the flag read: the interval is entered for as long as the child
  lives, instead of only while a stop scheduled milliseconds out has not
  arrived yet.

The same objection reaches past coverage to the **verdict**, and the
end-to-end tests answer it the same way: a poll waiting on a detached driver
is bounded by *consecutive probes that saw no change in the workspace tree*,
never by wall time (`src/e2e/poll.rs`, `docs/ARCHITECTURE.md` §9). A live
driver writes continuously and a wedged one writes nothing, so a loaded box
only makes the pass path slower — where a stopwatch would have turned a slow
success red, and (as bl-2bf0 found) hid a real defect behind a timeout that
read like machine load.

There is no `--no-verify` escape hatch in the workflow. If the hook rejects a
commit, fix the underlying issue rather than skipping.

### Auto-push hook

`.githooks/reference-transaction` pushes `main` to `origin` the moment local
`main` advances. Landing and publishing are one act: a `bl close` reaches
GitHub and the push triggers the Release-plz workflow, which contains CI as a
called job (`needs: ci`) and only publishes once it is green. `origin/main`
cannot silently fall months behind local `main` again.

**Why a `reference-transaction` hook and not `post-commit`.** Nothing lands on
this repo's `main` through `git commit`. `bl close` delivers by plumbing —
`git commit-tree`, then `git update-ref refs/heads/main` — which fires no
commit hook and no merge hook at all — every commit `bl` has landed on `main`
arrived that way. Git's `reference-transaction` hook is the one event every
landing path shares: the plumbing delivery, a `git merge --no-ff`, and a plain
commit alike all end in an update of `refs/heads/main`.

The hook acts only on the `committed` state of a transaction that moves
`refs/heads/main` to a new value, and only when an `origin` remote exists.
Everything else — side branches, `refs/remotes/*` (including the ones its own
push writes, so it cannot recurse), no-op rewrites like `git pack-refs`, and a
deletion of `main` — falls through untouched.

It cannot block or hang a landing. Git aborts a ref transaction when this hook
exits non-zero in the `prepared` state, so every path in it exits 0 — which is
also why it does not `set -e`. A push that fails prints one warning line on
stderr and nothing else, and `timeout 30` bounds an offline push rather than
stalling the commit behind a TCP timeout. Git runs `reference-transaction`
hooks from 2.28 onward; on anything older the file is simply never invoked and
`main` has to be pushed by hand.

`tests/hooks.rs` exercises the shipped hook file itself against a local bare
repository as `origin` — never the real remote — and covers all six behaviours
above: a commit on `main` pushes, a `commit-tree` + `update-ref` delivery
pushes, a `--no-ff` merge pushes, a side-branch commit pushes nothing, an
unreachable `origin` warns without failing the commit, and a repo with no
`origin` is silent.

### Commit-identity guard (opt-in, per machine)

`main`'s history carries exactly one human identity, `mudbungie
<mudbungie@gmail.com>`, and no `Co-Authored-By` trailers — it was normalized to
that on 2026-07-26. `tests/commit_hygiene.rs` keeps it that way, but only on a
machine that asks for it: the test arms itself on the presence of
`$XDG_CONFIG_HOME/litany/enforce-commit-identity` (default
`~/.config/litany/enforce-commit-identity`), an empty marker file **outside** the
repo. Absent — the default in public CI and in every clone — the test returns
without asserting anything.

Armed, it walks all of `refs/heads/main` and fails on any commit whose author or
committer is neither `mudbungie <mudbungie@gmail.com>` nor
`github-actions[bot]` (the bot stays allowed: release-plz authors the release
commit as it), on any `Co-Authored-By` trailer, and on any mention of a
throwaway or personal address in an identity or a message. The policy lives in
the marker, not in the code: `rm` it and the guard is off, with no code edit and
no flag. Create it with `touch ~/.config/litany/enforce-commit-identity`.

## License

MIT. See [`LICENSE`](LICENSE).
