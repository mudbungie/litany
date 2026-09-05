.PHONY: all build release test test-install coverage lint leak-scan fmt fmt-check check smoke schemas new-workspace eval install-hooks install install-bz brazen-pin install-verify uninstall ci promote-changelog image image-scan mac-artifact clean

# Install location for `make install`. Defaults to the XDG-ish user-local
# convention; override for system-wide installs or packaging:
#   make install INSTALL_PREFIX=/usr/local
INSTALL_PREFIX ?= $(HOME)/.local
INSTALL_BIN    := $(INSTALL_PREFIX)/bin

# Harness root (ARCH §2.2): installation-global state, split by XDG
# lifetime into a config root (hand-edited declarations) and a data root
# (machine-populated pools). LITANY_HOME, if set, collapses BOTH to that
# one directory — at install time and at runtime alike. This mirrors the
# resolver policy in src/harness_root.rs exactly.
XDG_CONFIG_HOME    ?= $(HOME)/.config
XDG_DATA_HOME      ?= $(HOME)/.local/share
ifdef LITANY_HOME
LITANY_CONFIG_HOME := $(LITANY_HOME)
LITANY_DATA_HOME   := $(LITANY_HOME)
else
LITANY_CONFIG_HOME := $(XDG_CONFIG_HOME)/litany
LITANY_DATA_HOME   := $(XDG_DATA_HOME)/litany
endif

# Binaries that resolve via `PATH`: the harness CLI, the eval runner,
# and the eval harness driver (README "Run the suite"). The desktop
# frontend lives in its own repo (yog) and installs from there.
PATH_BINARIES     := litany agent-eval litany-eval-agent
# The provider adapter is brazen's `bz` (ARCH §4.4) — one binary for
# every provider, installed from crates.io at the exact version the
# litany crate links (the load-time version guard, §4.4). The pin's one
# home is the `brazen = "=<pin>"` dependency in Cargo.toml; this derives
# from it (the code-side guard derives from the same line).
BRAZEN_PIN        := $(shell sed -n 's/^brazen = "=\([^"]*\)"$$/\1/p' Cargo.toml)
# The harness-root skeleton (config-lifetime templates + machine-populated
# pools and trees, ARCH §2.2) is founded by `litany prime`, invoked below —
# the single source of truth for what a ready installation carries. The
# Makefile no longer enumerates the subdirs or re-copies the pools.

all: check

build:
	cargo build --workspace

release:
	cargo build --workspace --release

# Test determinism: the pinned adapter, not whatever `bz` is on PATH.
#
# The e2e tests exec the REAL `bz`, and the load-time version guard (§4.4)
# demands the pin EXACTLY. Resolving it from `PATH` made every test run
# depend on machine-global mutable state (`~/.cargo/bin/bz`): one agent
# installing a different brazen version failed another worktree's gate
# on five-plus e2e tests, indistinguishable at a glance from a code
# regression. So the test targets below resolve `bz` from a cache keyed on
# the pin and put that directory FIRST on `PATH` — a worktree's tests always
# run the worktree's pin, whatever the machine's `bz` happens to be. This is
# test determinism only: runtime resolution for real use (§4.4 — `adapter:`
# override, injected target, else `bz` on `PATH`) is untouched, and so is
# `make install`, which still puts the pinned `bz` on the user's `PATH`.
#
# The version is BRAZEN_PIN above — Cargo.toml's `brazen = "="` line, the
# number's one home, the same line the code-side guard reads. The directory
# is NAMED after it, so a pin bump is a cache miss and nothing else; nothing
# is ever overwritten in place, which is what keeps a shared cache safe for
# parallel worktrees.
#
# Cost: cold, one `cargo install` per pin per machine (the cache is under
# XDG_CACHE_HOME, so sibling worktrees share it and only the first pays).
# Warm, one `stat` — the recipe is a file target, so make skips it outright.
XDG_CACHE_HOME ?= $(HOME)/.cache
BZ_TEST_ROOT   := $(XDG_CACHE_HOME)/litany/bz/$(BRAZEN_PIN)
BZ_TEST_PATH   := $(BZ_TEST_ROOT)/bin:$(PATH)

$(BZ_TEST_ROOT)/bin/bz:
	@test -n "$(BRAZEN_PIN)" || { echo 'could not derive the brazen pin from Cargo.toml (expected a `brazen = "=<version>"` line)' >&2; exit 1; }
	@echo "test adapter: installing bz $(BRAZEN_PIN) into $(BZ_TEST_ROOT)"
	@cargo install brazen --version "=$(BRAZEN_PIN)" --locked --root "$(BZ_TEST_ROOT)"

# Test determinism, second axis: the machine's git configuration.
#
# The suite spawns real `git` everywhere — `RealGit`, the `litany` binary
# under `litany new`, and the e2e fixtures — with a synthetic identity
# (`user.email=t@t`, `GIT_AUTHOR_EMAIL=test@example.invalid`) so a commit
# a test makes is hermetic. `~/.gitconfig` is machine-global mutable
# state exactly as `~/.cargo/bin/bz` was: a global `core.hooksPath`, a
# commit-msg hook enforcing the operator's own identity, a `commit.gpgsign`
# — any of them reaches into every spawned git and vetoes those commits,
# failing dozens of tests at once in a voice ("commit refused: AUTHOR
# email is <test@example.invalid>") indistinguishable at a glance from a
# code regression. One operator dotfile change should not decide whether
# this repo's suite passes.
#
# So the test targets run git against a generated global config carrying
# one thing — a synthetic identity, the fallback for the tests that never
# set one — and no system config at all. Repo-local and per-command
# settings still win over it, so a test that pins its own author is
# unaffected; what stops reaching the suite is everything the operator's
# file says beyond identity. Runtime behaviour for real use is untouched:
# this appears on the test recipes only, never on `install`, `smoke`, or
# anything a user runs.
TEST_GIT_CONFIG := $(CURDIR)/target/test-gitconfig
TEST_GIT_ENV    := GIT_CONFIG_GLOBAL=$(TEST_GIT_CONFIG) GIT_CONFIG_SYSTEM=/dev/null

$(TEST_GIT_CONFIG):
	@mkdir -p $(dir $@)
	@printf '[user]\n\tname = litany-test\n\temail = test@litany.invalid\n' > $@

test: $(BZ_TEST_ROOT)/bin/bz $(TEST_GIT_CONFIG)
	$(TEST_GIT_ENV) PATH="$(BZ_TEST_PATH)" cargo test --workspace

# The install contract end-to-end (tests/install.rs). It shells out to
# `make install` — a release build plus `cargo install brazen` — which
# contends with tarpaulin's `target/` lock, so the test carries
# `cfg_attr(tarpaulin, ignore)` and `make coverage` never runs it. This
# target is where it runs instead: uninstrumented, and part of `check`
# below, so the pre-commit/close gate exercises the first thing every
# user touches — including the `include_dir!` embedded-asset seam
# (src/install.rs, src/template/mod.rs) as a real release binary sees it.
# It is the tree's only tarpaulin-ignored test; a future sibling belongs
# on this line, not in a new target.
#
# Cost: ~45s warm. The test writes NO machine-global state: it runs the
# real `install-bz` below, but with `CARGO_INSTALL_ROOT` pointed at a
# per-worktree root under `target/`, so the pinned `bz` lands there
# instead of on the user's cargo bin (tests/install.rs::bz_install_root).
# The isolation lives on the test's own `make` invocation, not in a
# test-only branch here, so it holds under `cargo test` and `make test`
# alike and `make install` for a user is unchanged.
test-install: $(TEST_GIT_CONFIG)
	$(TEST_GIT_ENV) cargo test --test install

TARPAULIN_PIN := 0.35.2

coverage: $(BZ_TEST_ROOT)/bin/bz $(TEST_GIT_CONFIG)
	@have=$$(cargo tarpaulin --version 2>/dev/null | awk '{print $$NF}'); \
	if [ "$$have" != "$(TARPAULIN_PIN)" ]; then \
	  echo "tarpaulin $(TARPAULIN_PIN) required (have: $${have:-none}); see tarpaulin.toml" >&2; \
	  echo "  cargo install cargo-tarpaulin --version $(TARPAULIN_PIN) --locked" >&2; \
	  exit 1; \
	fi
	$(TEST_GIT_ENV) PATH="$(BZ_TEST_PATH)" cargo tarpaulin --workspace --fail-under 100 --skip-clean --out Stdout --exclude-files 'src/bin/*' --exclude-files 'src/bin/litany/*' --exclude-files 'src/e2e/*' --exclude-files 'crates/*/src/main.rs'

# Regenerate schemas/ from the crate's schema types. The generator is the
# `config::schemas` module; `make schemas` drives it through the in-crate
# golden test's update flow (UPDATE_SCHEMAS=1 rewrites schemas/ in place
# instead of asserting byte-identity). The same test, run without the env
# var, is the CI guard that schemas/ never drifts from the source.
schemas:
	UPDATE_SCHEMAS=1 cargo test --quiet --lib schemas_golden

new-workspace:
	@test -n "$(DEST)" || { echo "usage: make new-workspace DEST=<path>"; exit 1; }
	@cargo run --quiet --bin litany -- new "$(DEST)"

# Run the evaluation runner (ARCH §9.3): experiments × suite × N.
#   make eval CONFIG=baseline SUITE=tests/suite RUNS=5 AGENT=target/debug/litany-eval-agent
#   make eval CONFIG="baseline single-attempt" SUITE=tests/suite RUNS=5 AGENT=litany-eval-agent
# CONFIG takes one or more experiment names (bl-f838): several run the
# same suite under each — the first is the comparison's baseline — and
# print the baseline → candidate comparison per later variant.
# AGENT is REQUIRED and has no default: which driver runs the agent under
# test (the §9.3 agent seam) is an experiment-defining input — a hidden
# default would silently bind every measurement to it. The shipped driver
# is `litany-eval-agent` (built here; installed on PATH by `make install`);
# it execs `litany` from PATH per run, so have `make install` done first.
# Any program honouring the contract in README "Run the suite" works.
# RECORD=<path> (optional) saves the machine-readable evaluation record
# (bl-36fa) — the input `agent-eval compare <baseline> <candidate>` takes;
# with several CONFIG names it is a directory, one `<experiment>.json`
# per variant. A live-model eval is always this explicit operator
# command, never CI.
eval:
	@test -n "$(CONFIG)" -a -n "$(AGENT)" || { \
	  echo "usage: make eval CONFIG=\"<experiment>...\" SUITE=<dir> RUNS=<n> AGENT=<driver-cmd> [RECORD=<path>]"; \
	  echo "AGENT is required; the shipped driver is litany-eval-agent (README, \"Run the suite\")"; \
	  exit 1; }
	@cargo build --quiet -p litany-eval-agent
	@cargo run --quiet -p agent-eval -- run $(foreach c,$(CONFIG),--config "$(c)") --suite "$(SUITE)" --runs "$(RUNS)" --agent "$(AGENT)" $(if $(RECORD),--record "$(RECORD)")

# The disclosure gate (scripts/leak-rules.sh is the table, leak-scan.sh the
# mechanism; from rust-bootstrap bl-2c4e). --self-test first: a leak gate dies
# by silently matching nothing. The machine-global bl-leak-gate plugin runs
# this same scanner over the TASK STORE before every bl publish.
leak-scan:
	@scripts/leak-scan.sh --self-test
	@scripts/leak-scan.sh

lint: leak-scan
	cargo clippy --all-targets -- -D warnings

fmt:
	cargo fmt

fmt-check:
	cargo fmt --check

check: fmt-check lint coverage test-install

# `make smoke` — the live-wire smoke test (README "First-run smoke test").
# The FIRST real model call the project makes: `litany new` + one live
# `litany prompt` against the SHIPPED defaults (worker role, anthropic /
# claude-sonnet-5) through the real `bz` data plane, with the verdict read
# from observable state (exit 0, a committed transcript on the agent ref, a
# step record with no wire error and real assistant text). Deliberately NOT
# in `check` or the close gate: it needs a configured `bz` credential for
# the anthropic provider (`bz --login --provider anthropic`, or set
# ANTHROPIC_API_KEY / BRAZEN_API_KEY) and spends money. It is the only check
# that proves the authored default model id resolves on the wire — `make
# check` mocks the wire and structurally cannot.
#
# Override the target with BOTH SMOKE_PROVIDER and SMOKE_MODEL (both-or-
# neither; unset => the shipped default above, credential note included):
#   make smoke SMOKE_PROVIDER=local SMOKE_MODEL=<a-pulled-ollama-model>
#   make smoke SMOKE_PROVIDER=codex SMOKE_MODEL=gpt-5.4
# Local ollama needs no credential, only a served model; the credential
# note applies to the anthropic default alone.
smoke:
	@cargo build --quiet --bin litany
	@LITANY_BIN="$(CURDIR)/target/debug/litany" \
	 SMOKE_PROVIDER="$(SMOKE_PROVIDER)" SMOKE_MODEL="$(SMOKE_MODEL)" \
	 bash scripts/smoke.sh

install-hooks:
	git config core.hooksPath .githooks
	@echo "hooks: core.hooksPath -> .githooks"

# Print the brazen pin on stdout, and nothing else. Exists so a consumer
# that needs the version as a *value* reads it from the pin's one home
# (the `brazen = "="` line in Cargo.toml, via BRAZEN_PIN above) instead
# of copying the number: `.github/workflows/ci.yml` keys its `bz` cache
# on `make brazen-pin`, so bumping the dependency bumps the cache too and
# no workflow file ever names a version.
brazen-pin:
	@echo "$(BRAZEN_PIN)"

# Install the provider adapter `bz` (ARCH §4.4) at the pinned version.
# Idempotent and cheap: a no-op when the `bz` already on PATH reports the
# pin, so a warm CI cache and a re-run of `make install` both cost
# nothing. The load-time version guard demands an EXACT match, so a `bz`
# at any other version — newer included — is replaced, not kept.
#
# This is the USER-facing install, not a test prerequisite: `make test`
# and `make coverage` feed the e2e tests their own pin-keyed `bz` (see
# BZ_TEST_ROOT above) and no longer care what is on the cargo bin.
install-bz:
	@test -n "$(BRAZEN_PIN)" || { echo 'could not derive the brazen pin from Cargo.toml (expected a `brazen = "=<version>"` line)' >&2; exit 1; }
	@have=$$(bz --version 2>/dev/null | awk '{print $$NF}'); \
	if [ "$$have" = "$(BRAZEN_PIN)" ]; then \
	  echo "provider adapter: bz $(BRAZEN_PIN) already on PATH"; \
	else \
	  echo "installing the provider adapter: cargo install brazen --version =$(BRAZEN_PIN)"; \
	  cargo install brazen --version "=$(BRAZEN_PIN)" --locked; \
	fi

# `make install` lays down the harness root skeleton on first run and is
# idempotent on subsequent runs. The binaries built from this tree are
# re-installed unconditionally (a fresh build is the point) while the
# crates.io-pinned `bz` is left alone when it already matches the pin
# (see `install-bz`); config files are guarded with `test -e` so
# rotated credentials and hand-edited entries survive a re-install.
install: release
	@mkdir -p "$(INSTALL_BIN)"
	@for bin in $(PATH_BINARIES); do \
		install -m 0755 "target/release/$$bin" "$(INSTALL_BIN)/$$bin"; \
		echo "installed $(INSTALL_BIN)/$$bin"; \
	done
	@$(MAKE) --no-print-directory install-bz
	@# Found the harness root via the freshly-installed binary (ARCH §2.2):
	@# `litany prime` seeds the default models.yaml, the tool/skill pools,
	@# and the workflows/ + workspaces/ dirs, seed-if-absent throughout —
	@# hand-edited config survives, and the seeding lives in one place (the
	@# verb), not duplicated here. The env below mirrors this Makefile's
	@# root resolution; the binary applies the identical policy (§2.2).
	@# It reports both roots and its seed-if-absent split on stderr itself
	@# (bl-7e9e), so nothing is echoed here: the resolution the verb used is
	@# the authoritative one, and a second copy could only ever disagree.
	@LITANY_HOME='$(LITANY_HOME)' XDG_CONFIG_HOME='$(XDG_CONFIG_HOME)' XDG_DATA_HOME='$(XDG_DATA_HOME)' "$(INSTALL_BIN)/litany" prime
	@$(MAKE) --no-print-directory install-verify
	@echo
	@echo "make sure $(INSTALL_BIN) is on your PATH (and that 'bz' resolves there too)."
	@echo "config root: $(LITANY_CONFIG_HOME)   data root: $(LITANY_DATA_HOME)"
	@echo "  (LITANY_HOME collapses both; else \$$XDG_CONFIG_HOME / \$$XDG_DATA_HOME)"
	@echo "provider endpoints/auth live in brazen's config: bz --dump-config / bz --login."
	@echo "pick each role's provider row + model id in a repo's providers.yaml (or the"
	@echo "  $(LITANY_CONFIG_HOME)/template/ override) — see ARCH §4.3/§4.4."

# Smoke-test the freshly installed binaries: `litany --version` proves the
# CLI loads, `litany new` exercises workspace creation (bare repo.git +
# first config commit, ARCH §2.2) against a throwaway path. Failure here
# aborts `make install` with a non-zero exit, since a half-installed
# harness is worse than none.
install-verify:
	@tmp=$$(mktemp -d) && trap "rm -rf $$tmp" EXIT && \
		"$(INSTALL_BIN)/litany" --version >/dev/null && \
		"$(INSTALL_BIN)/litany" new "$$tmp/test" >/dev/null && \
		echo "verify: litany --version + litany new ok"

uninstall:
	@for bin in $(PATH_BINARIES); do \
		rm -f "$(INSTALL_BIN)/$$bin" && echo "removed $(INSTALL_BIN)/$$bin"; \
	done
	@echo "note: 'bz' (brazen) was installed via cargo; remove with 'cargo uninstall brazen'."

ci: check

# The release step release-plz no longer performs (changelog_update = false in
# release-plz.toml — CHANGELOG.md is hand-maintained, the rationale is in that
# file's header, bl-7558): stamp the accumulated `## [Unreleased]` section as
# the released version, with compare link and date, and open a fresh empty
# [Unreleased] above it. VERSION is the version the open release PR proposes —
# the PR is the authority, this target just names what it says. Run it in a
# task worktree and land it on main; that landing IS the release act, because
# release-plz.yml's merge-release-pr job holds the release PR until the
# changelog names the version it proposes and merges it once one does (worktree
# discipline is CLAUDE.md's; the ordering keeps the tagged tree's changelog
# already stamped, and it is now a gate rather than a convention). Refuses a
# VERSION this ERA already has, so a re-run is a no-op failure, not a
# duplicate section.
#
# Both halves are era-aware, and they must stay so (bl-4afc). CHANGELOG.md
# spans the bl-2f58 rename fence: it carries lernie-era headings 0.0.1 through
# 0.0.11 AND a litany era that restarted at 0.0.1, so a bare `## [x.y.z]`
# number names two different releases. The compare URL is what tells them
# apart — a litany-era heading links into THIS repo, a lernie-era one into
# `mudbungie/lernie` — so COMPARE_URL below is the era predicate, used once as
# the duplicate guard and once to read the previous version. A guard on the
# bare number refused every version up to 0.0.11 forever; a prev-extraction on
# the bare number would read a lernie-era heading as this era's predecessor.
COMPARE_URL := https://github.com/mudbungie/litany/compare
# The tag spelling on this side of the fence, matching release-plz.toml's
# `git_tag_name = "litany-v{{ version }}"` — the bare `v<version>` tags belong
# to the lernie era and are already taken. Keep the two in step.
TAG_PREFIX  := litany-v
promote-changelog:
	@test -n "$(VERSION)" || { echo "usage: make promote-changelog VERSION=x.y.z"; exit 1; }
	@! grep -q '^## \[$(VERSION)\]($(COMPARE_URL)/' CHANGELOG.md || { echo "CHANGELOG.md already has a litany-era [$(VERSION)]"; exit 1; }
	@prev=$$(sed -n 's|^## \[\([0-9][^]]*\)\]($(COMPARE_URL)/.*|\1|p' CHANGELOG.md | head -1); \
	test -n "$$prev" || { echo "no previous litany-era '## [x.y.z]($(COMPARE_URL)/...)' heading in CHANGELOG.md"; exit 1; }; \
	sed -i 's|^## \[Unreleased\]$$|## [Unreleased]\n\n## [$(VERSION)]($(COMPARE_URL)/$(TAG_PREFIX)'"$$prev"'...$(TAG_PREFIX)$(VERSION)) - '"$$(date +%F)"'|' CHANGELOG.md
	@echo "promoted [Unreleased] -> [$(VERSION)]"

# The OCI image — the unit of install for a box that takes containers rather
# than binaries. `Containerfile` is the whole of what it builds and states why
# each layer is what it is; this target only decides the engine and the tag.
#
# The version is READ FROM Cargo.toml and never typed here, for the same reason
# BRAZEN_PIN above is derived rather than restated: a version typed into a
# second file is a version that drifts. Both `:<version>` and `:latest` are
# applied to the same build.
#
# Podman or docker, whichever the box has, podman first — it needs no daemon
# and no group membership, which is the difference between "the operator can
# build this" and "the operator can build this once an admin says yes".
# Override with `make image CONTAINER_ENGINE=docker`.
#
# IT PUSHES NOTHING, and there is deliberately no `push` target — the same
# reasoning that keeps `publish` out of this Makefile's hands. The registry is
# now named (`ghcr.io/mudbungie/litany`, yog DESIGN §10.1, operator ruling
# 2026-08-30) and the push still does not live here: it belongs to the release
# workflow at tag time, where the publishing identity exists and nowhere else.
# A push is not undoable — a tag can move, but the bytes anyone pulled are
# theirs — and a convenience target for an irreversible act is how the act
# happens by accident.
#
# The `:latest` tag applied below is LOCAL, and that is a different act from a
# published one: a local tag is a convenience on one box nobody else can pull,
# while a published `latest` is a name whose bytes change under everyone who
# ever wrote it down. The registry gets the version and the digest, both
# immutable, and never a moving `latest`.
IMAGE_NAME       ?= litany
IMAGE_VERSION    := $(shell sed -n '/^\[package\]/,/^\[/{s/^version *= *"\([^"]*\)".*/\1/p;}' Cargo.toml)
IMAGE_TAG        := $(IMAGE_NAME):$(IMAGE_VERSION)
CONTAINER_ENGINE ?= $(shell command -v podman 2>/dev/null || command -v docker 2>/dev/null)

image:
	@test -n "$(CONTAINER_ENGINE)" || { echo "image: no podman and no docker on PATH" >&2; exit 1; }
	@test -n "$(IMAGE_VERSION)" || { echo "image: no version in Cargo.toml" >&2; exit 1; }
	@echo "image: $(notdir $(CONTAINER_ENGINE)) build -> $(IMAGE_TAG)"
	@$(CONTAINER_ENGINE) build -f Containerfile \
	  -t "$(IMAGE_TAG)" -t "$(IMAGE_NAME):latest" .
	@$(CONTAINER_ENGINE) image inspect "$(IMAGE_TAG)" \
	  --format 'image: {{.Id}} {{.Size}} bytes'
	@$(MAKE) --no-print-directory image-scan

# The image-side disclosure gate (yog DESIGN §10.1's condition on the registry
# ruling). `make leak-scan` reads the git INDEX; an image is built from inputs
# no commit has — the build context as the engine receives it, the base layers,
# the package index, and the image CONFIG — so nothing in the source gate has
# ever read a byte of what a push would publish.
#
# It is a step of `image` and not a target beside it, for the reason the
# pre-commit hook is not a target beside `commit`: a gate a person has to
# remember to run is not a gate. Run it alone to re-judge an image already
# built. `scripts/image-scan.sh` states what it scans and how it isolates the
# authored content; this target only decides which tag and runs BOTH
# directions — the planted-secret self-test first, because a scan that has
# stopped matching passes everything forever, then the real image.
image-scan:
	@test -n "$(CONTAINER_ENGINE)" || { echo "image-scan: no podman and no docker on PATH" >&2; exit 1; }
	@test -n "$(IMAGE_VERSION)" || { echo "image-scan: no version in Cargo.toml" >&2; exit 1; }
	@CONTAINER_ENGINE=$(CONTAINER_ENGINE) scripts/image-scan.sh --self-test "$(IMAGE_TAG)"
	@CONTAINER_ENGINE=$(CONTAINER_ENGINE) scripts/image-scan.sh "$(IMAGE_TAG)"

# The macOS artifacts — the aarch64 mac `litany` AND the `bz` at the pin,
# cross-produced from a Linux container so they come off the same reproducible
# line as the Linux image rather than off somebody's laptop (yog DESIGN §10,
# README "The macOS artifact"). `Containerfile.mac` is the whole of what it
# builds and argues the toolchain choice — `zig cc`, with osxcross refused on
# Apple's own licence terms — where the decision lives.
#
# BOTH BINARIES, for the reason the image ships both: `bz` is not optional and
# a mac artifact that was only the engine would be an install route that cannot
# answer a prompt.
#
# THE PRODUCT IS TWO FILES, NOT AN IMAGE. The build's last stage is `FROM
# scratch` carrying them, so `create` + `cp` lifts them out without running
# anything; the wrapper image is a fixture and is deleted below. That is why
# `image-scan` is not wired in here and is not being skipped: the image is
# never pushed, and the artifacts are compiled from the same tree the source
# gate reads, exactly as the Linux release binaries are.
#
# IT IS VERIFIED, NOT ASSUMED. No mac exists on the build box, so the artifacts
# cannot be executed — but they can be READ, and a green build is not evidence
# of an arm64 Mach-O that a mac would load. `scripts/mac-verify.sh` reads the
# header, LC_BUILD_VERSION and every LC_LOAD_DYLIB out of each produced file,
# and runs its own negative direction first (five malformed inputs it must
# refuse), the same two-direction discipline `leak-scan` holds.
#
# NOT part of `check`, for the reason `image` is not: `check` must run on a box
# with no container engine, and this needs one.
MAC_TARGET := aarch64-apple-darwin
MAC_IMAGE  := $(IMAGE_NAME)-macos-build:$(IMAGE_VERSION)
MAC_DIST   := dist/$(MAC_TARGET)

mac-artifact:
	@test -n "$(CONTAINER_ENGINE)" || { echo "mac-artifact: no podman and no docker on PATH" >&2; exit 1; }
	@test -n "$(IMAGE_VERSION)" || { echo "mac-artifact: no version in Cargo.toml" >&2; exit 1; }
	@scripts/mac-verify.sh --self-test
	@echo "mac-artifact: $(notdir $(CONTAINER_ENGINE)) build -> $(MAC_TARGET)"
	@$(CONTAINER_ENGINE) build -f Containerfile.mac -t "$(MAC_IMAGE)" .
	@mkdir -p "$(MAC_DIST)"
	@cid=$$($(CONTAINER_ENGINE) create "$(MAC_IMAGE)") && \
	  $(CONTAINER_ENGINE) cp "$$cid:/litany" "$(MAC_DIST)/litany" && \
	  $(CONTAINER_ENGINE) cp "$$cid:/bz" "$(MAC_DIST)/bz" && \
	  $(CONTAINER_ENGINE) rm "$$cid" >/dev/null
	@$(CONTAINER_ENGINE) rmi "$(MAC_IMAGE)" >/dev/null
	@chmod +x "$(MAC_DIST)/litany" "$(MAC_DIST)/bz"
	@scripts/mac-verify.sh "$(MAC_DIST)/litany"
	@scripts/mac-verify.sh "$(MAC_DIST)/bz"

clean:
	cargo clean
	rm -rf dist
