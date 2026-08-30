#!/usr/bin/env bash
# Live-wire smoke test — the one check `make check` structurally cannot make.
#
# `make check` runs the whole test suite against a MOCKED wire (httpmock
# Anthropic SSE), so it can never catch a shipped default that fails on the
# real provider — which is exactly how the fake worker model id
# `claude-sonnet-4-7` shipped unnoticed (bl-3157). This script makes the
# first real model call: `litany new` scaffolds a throwaway workspace, then
# one live `litany prompt` runs against the SHIPPED defaults (worker role,
# provider `anthropic`, model `claude-sonnet-5`) through the real `bz` data
# plane. It is deliberately NOT part of `make check` or the close gate: it
# needs a configured provider credential and spends money.
#
# Default target: the shipped default is `anthropic` / `claude-sonnet-5`,
# and running it needs a `bz` anthropic credential (`bz --login --provider
# anthropic`, or export `ANTHROPIC_API_KEY` / `BRAZEN_API_KEY`).
#
# Provider/model override: set BOTH `SMOKE_PROVIDER` and `SMOKE_MODEL`
# (both-or-neither; one alone is a usage error) to run the same live
# `litany prompt` against any `bz` provider row instead of the shipped
# default. Unset leaves today's behavior byte-for-byte. The override is
# laid into the throwaway config root through the same front door a real
# install uses — a `providers.yaml` override in `<config-root>/template/`
# (the bl-e795 config-root override; the role assignment is the whole
# model binding, bl-35e2) — so no new `litany` flag or verb exists. Local
# ollama needs no credential, only a served model:
#   make smoke SMOKE_PROVIDER=local  SMOKE_MODEL=<a-pulled-ollama-model>
#   make smoke SMOKE_PROVIDER=codex  SMOKE_MODEL=gpt-5.4
#
# The verdict is read from OBSERVABLE STATE, never the agent's own claim.
# A subtle trap discovered building this: an auth-failed run STILL creates
# the agent branch and a step record whose response.json terminates in a
# clean `{"type":"end"}` — the failure rides an `{"type":"error"}` line
# ahead of it. So branch-exists / step-exists / terminal-end are necessary
# but NOT sufficient. The teeth of this test are: prompt exit 0, AND no
# `"type":"error"` line in any response, AND at least one real assistant
# `content_delta` — proof a model on the wire actually answered.
#
# Inputs (set by the `make smoke` recipe):
#   LITANY_BIN               the freshly built litany binary (absolute)
#   SMOKE_PROVIDER/SMOKE_MODEL  optional override target (both or neither)
#
# The harness root the workspace scaffolds against is founded by `litany
# prime` (ARCH §2.2) from assets EMBEDDED in that binary — the same front
# door `make install` uses — so this smoke exercises the shipped install
# path end to end (prime -> new -> prompt), not a hand-copied source tree.
set -euo pipefail

# Scrub inherited git env (a set GIT_DIR silently redirects every `git -C`).
unset GIT_DIR GIT_WORK_TREE GIT_INDEX_FILE GIT_OBJECT_DIRECTORY \
      GIT_PREFIX GIT_COMMON_DIR GIT_ALTERNATE_OBJECT_DIRECTORIES 2>/dev/null || true

: "${LITANY_BIN:?set LITANY_BIN}"

# Provider/model override (both-or-neither). Unset => the shipped default,
# byte-for-byte. One alone is a usage error — half an override would
# silently resolve against a mismatched default and mislead.
SMOKE_PROVIDER="${SMOKE_PROVIDER:-}"
SMOKE_MODEL="${SMOKE_MODEL:-}"
if { [ -n "$SMOKE_PROVIDER" ] && [ -z "$SMOKE_MODEL" ]; } \
   || { [ -z "$SMOKE_PROVIDER" ] && [ -n "$SMOKE_MODEL" ]; }; then
  echo "smoke: usage — set BOTH SMOKE_PROVIDER and SMOKE_MODEL, or NEITHER" >&2
  echo "  unset => the shipped default (anthropic / claude-sonnet-5)" >&2
  echo "  e.g. make smoke SMOKE_PROVIDER=local SMOKE_MODEL=<a-pulled-ollama-model>" >&2
  exit 2
fi
if [ -n "$SMOKE_PROVIDER" ]; then
  TARGET="the override ($SMOKE_PROVIDER / $SMOKE_MODEL)"
else
  TARGET="the shipped default (anthropic / claude-sonnet-5)"
fi

fail() { echo "smoke: FAIL — $1" >&2; echo "  (workspace kept for inspection: $ROOT)" >&2; exit 1; }

ROOT="$(mktemp -d "${TMPDIR:-/tmp}/litany-smoke.XXXXXX")"
HOME_DIR="$ROOT/home"
WS="$ROOT/ws"

# When an override is set, lay it into the throwaway config root BEFORE
# `litany prime`, through the front door a real install already uses — no
# new `litany` flag or verb. One config file selects the worker's target:
# per-repo `providers.yaml` names the role's provider ROW + model id
# (config-commit control, overridable via `<config-root>/template/`, the
# bl-e795 override). That assignment is the whole binding (bl-35e2): the
# global `models.yaml` carries no models table, and the id's validity is
# exactly what the live call below verifies.
if [ -n "$SMOKE_PROVIDER" ]; then
  mkdir -p "$HOME_DIR/template"
  cat > "$HOME_DIR/template/providers.yaml" <<YAML
roles:
  worker:
    provider: $SMOKE_PROVIDER
    model: $SMOKE_MODEL
    tools: []
  compactor:
    provider: $SMOKE_PROVIDER
    model: $SMOKE_MODEL
YAML
fi

# Found the harness root through the verb — the single source of truth for
# what a ready installation carries (models.yaml + tool/skill pools, ARCH
# §2.2), seeded from assets embedded in the binary. This is exactly what
# `make install` does, so `make smoke` covers the real shipped path.
export LITANY_HOME="$HOME_DIR"
"$LITANY_BIN" prime || fail "litany prime exited non-zero"

echo "smoke: scaffolding a throwaway workspace ($WS)" >&2
"$LITANY_BIN" new "$WS" >/dev/null || fail "litany new exited non-zero"

echo "smoke: one live 'litany prompt' against $TARGET" >&2
set +e
ID="$("$LITANY_BIN" prompt "$WS" 'Reply with exactly one word: pong')"
PROMPT_EXIT=$?
set -e
[ "$PROMPT_EXIT" -eq 0 ] || fail "litany prompt exited $PROMPT_EXIT (the wire rejected $TARGET)"
[ -n "$ID" ] || fail "litany prompt printed no agent id"
case "$ID" in */*) fail "agent id contains a slash: $ID";; esac

BARE="$WS/repo.git"
BRANCH="agents/$ID"

# Observable check 1: the agent ref exists and carries a transcript commit.
git -C "$BARE" show-ref --verify --quiet "refs/heads/$BRANCH" \
  || fail "agent ref $BRANCH does not exist"
git -C "$BARE" log --format=%s "$BRANCH" | grep -q '^transcript ' \
  || fail "no committed transcript entry on $BRANCH"

# Observable check 2: a step record exists off-worktree (ARCH §2.2).
STEP="$WS/steps/$ID/001"
[ -f "$STEP/request.json" ] || fail "no step request.json at $STEP"
[ -f "$STEP/response.json" ] || fail "no step response.json at $STEP"

# Observable check 3 (the teeth): no error anywhere in the run's responses,
# and a real assistant content_delta landed. A metered wire answered.
if grep -l '"type":"error"' "$WS"/steps/"$ID"/*/response.json >/dev/null 2>&1; then
  fail "a response carries a wire error line ($(grep -h '"type":"error"' "$WS"/steps/"$ID"/*/response.json | head -1))"
fi
grep -q '"type":"content_delta"' "$WS"/steps/"$ID"/*/response.json \
  || fail "no assistant content_delta — the model produced no visible text"

echo "smoke: PASS — id=$ID, $(ls -d "$WS"/steps/"$ID"/*/ | wc -l) step(s), assistant text on the wire" >&2
rm -rf "$ROOT"
