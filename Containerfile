# litany as an OCI image — the unit of install, and nothing more.
#
# The image is a DEPLOYMENT artifact. Nothing in litany uses the container
# filesystem as a feature and no harness state lives in a layer: the XDG roots
# are the runtime contract and they are mounted in. Read the README section
# "As a container image" for what mounts where.
#
# Two stages. The build stage is the pinned toolchain and the C toolchain the
# adapter's TLS stack needs; the runtime stage is the small set of programs the
# engine actually EXECS, and nothing else.

# ---------------------------------------------------------------------------
# Stage 1 — build, under the toolchain rust-toolchain.toml pins.
#
# `rust:<pin>-alpine` and not `-slim-bookworm`, because the host target of the
# alpine image IS `x86_64-unknown-linux-musl`: both binaries come out
# statically linked with no cross-compilation setup and no `--target` flag to
# keep in step with anything. The tag is digest-pinned so a rebuild resolves
# the same bytes; the tag beside it is for a human reading the line.
FROM docker.io/library/rust:1.95.0-alpine3.22@sha256:064dfc925d68d1a63f4fd2871bd7dc6e6ea56692989a487185855d62885d90aa AS build

# `musl-dev` is not incidental. The provider adapter's TLS stack compiles C
# (`ring`), and so does nothing else in this build. It is confined to this
# stage; the runtime layer carries no compiler.
RUN apk add --no-cache musl-dev

WORKDIR /src

# The toolchain pin has ONE home — rust-toolchain.toml — and the `FROM` line
# above is a second statement of the same fact, so it can drift. This makes the
# drift a build failure rather than a silent difference between what the gate
# compiles and what the image ships.
#
# It is copied to /pin and not to the build directory on purpose: a
# rust-toolchain.toml in the working directory sends every later `cargo` and
# `rustc` through rustup's shim, which would try to DOWNLOAD the toolchain and
# the `components` list into an image that already has the compiler. The check
# reads the file; the build never sees it.
COPY rust-toolchain.toml /pin/rust-toolchain.toml
RUN set -eu; \
    pin=$(sed -n 's/^channel *= *"\([^"]*\)".*/\1/p' /pin/rust-toolchain.toml); \
    have=$(rustc --version | cut -d' ' -f2); \
    if [ "$pin" != "$have" ]; then \
      echo "Containerfile: base image rustc $have, rust-toolchain.toml pins $pin" >&2; \
      echo "  bump the FROM tag and its digest in lockstep with the pin" >&2; \
      exit 1; \
    fi

# The workspace manifests, the crate source, and the four embedded asset trees
# `include_dir!`/`include_str!` read at compile time — `template/`, `schemas/`,
# `skills/`, `install/models.yaml`. Their absence is a build failure, which is
# why they are named here beside `src/` rather than assumed.
#
# `crates/` is copied whole although only the `litany` binary is built: cargo
# resolves the workspace before it resolves the package, so a member manifest
# that is not there is an error before the build starts.
COPY Cargo.toml Cargo.lock ./
COPY crates ./crates
COPY src ./src
COPY template ./template
COPY schemas ./schemas
COPY skills ./skills
COPY install ./install

# `--locked` for the same reason the gate uses it: the committed Cargo.lock is
# the dependency answer, and a build allowed to solve for a different one is
# not the build the gate judged.
#
# Only the `litany` binary. `agent-eval` and `litany-eval-agent` are repo-side
# evaluation tooling that read this tree's `tests/suite/` — they have no
# meaning on a deployed box, and the README's install table already says the
# non-Makefile routes do not lay them down either.
RUN cargo build --release --locked --bin litany

# The provider adapter, at the pin. `bz` is NOT optional — the README's install
# table is explicit that every route needs it, litany resolves it on PATH, and
# a load-time guard rejects any `bz` whose version differs from the pin. An
# image that shipped the engine without the adapter would be an install route
# that cannot answer a prompt.
#
# The pin is READ from `Cargo.toml`'s `brazen = "="` line, which is its one
# home (the Makefile's BRAZEN_PIN reads the same line). It is not typed here,
# because a version typed into a second file is a version that drifts.
RUN set -eu; \
    pin=$(sed -n 's/^brazen = "=\([^"]*\)"$/\1/p' Cargo.toml); \
    test -n "$pin" || { echo 'Containerfile: no `brazen = "=<version>"` line in Cargo.toml' >&2; exit 1; }; \
    cargo install brazen --version "=$pin" --locked --root /adapter

# ---------------------------------------------------------------------------
# Stage 2 — runtime.
#
# THE RUNTIME LAYER IS WHAT THE ENGINE EXECS, and this engine execs four
# things. `FROM scratch` is wrong here whatever the linking story says:
#
#   git   — the harness is git-backed and shells to the `git` on PATH for
#           every workspace act (`src/template/mod.rs`). Without it litany can
#           create nothing, read nothing back, and land nothing.
#   sh    — the `bash` built-in tool runs `sh -c <command>`
#           (`src/prompt/tool/builtin/bash/mod.rs`). busybox provides it.
#   bz    — the provider adapter, copied from the build stage above.
#   litany— itself: the built-in tool set and dispatch re-exec this binary.
#
# `ca-certificates` is here because the adapter speaks HTTPS to a provider
# endpoint and `git` may be pointed at an HTTPS remote. It is the one thing on
# this list that is not exec'd but is still load-bearing.
#
# Everything past that list is the operator's. A tool the harness is configured
# to run that this layer does not have is a tool this box does not have; add it
# with `apk add` in a derived image rather than widening this one, so what the
# base promises stays exactly the four programs above.
FROM docker.io/library/alpine:3.22@sha256:14358309a308569c32bdc37e2e0e9694be33a9d99e68afb0f5ff33cc1f695dce

RUN apk add --no-cache git ca-certificates

COPY --from=build /src/target/release/litany /usr/local/bin/litany
COPY --from=build /adapter/bin/bz /usr/local/bin/bz

# WHAT MOUNTS WHERE. XDG is the runtime contract and the image carries no
# harness state, so this sets the two variables and provisions nothing under
# them. litany's roots are `$XDG_CONFIG_HOME/litany` and `$XDG_DATA_HOME/litany`
# (ARCH §2.2), which makes them `/config/litany` and `/state/litany` here — the
# extra level is XDG's, not the image's: both variables are parents of
# per-application roots by definition and an image does not get to collapse
# them. `LITANY_HOME` still overrides both at run time for an operator who
# would rather mount one directory.
#
# Nothing here runs `litany prime`. Seeding the harness root writes fifteen
# files, and writing them into a LAYER would put the one state litany owns
# where a mount cannot replace it and an upgrade cannot see it. `prime` is
# seed-if-absent and is the operator's first act against the mounted roots.
ENV XDG_CONFIG_HOME=/config \
    XDG_DATA_HOME=/state

# Workspaces are named by path on the command line and can live anywhere; the
# image asserts no location for them, only that whatever path is named has to
# be a mount if the workspace is to outlive the container.
WORKDIR /work

ENTRYPOINT ["/usr/local/bin/litany"]
CMD ["--help"]
