#!/usr/bin/env bash
# image scan — the disclosure gate for the OCI image, standing to `make image`
# exactly as `leak-scan.sh` stands to the commit.
#
#   scripts/image-scan.sh IMAGE        scan one built image
#   scripts/image-scan.sh --self-test IMAGE
#                                      plant fabricated secrets on top of that
#                                      image and prove each one is caught
#
# WHY A SECOND GATE AND NOT A REUSE OF THE FIRST. `make leak-scan` reads the
# git INDEX. An image is built from inputs no commit has: the build context as
# the engine actually receives it, the base image's layers, the package index,
# and the image CONFIG. Not one byte of that has ever been read by a gate, and
# a push is less recallable than a `cargo publish` — a tag can move, but the
# bytes anyone pulled are theirs. The condition on the registry ruling (yog
# `docs/DESIGN.md` §10.1) is that this runs before anything is pushed, wired
# into the build path rather than left beside it.
#
# THE RULE TABLE IS THE SAME TABLE. `scripts/leak-rules.sh` says what counts as
# a leak and `scripts/leak-scan.sh` is the mechanism that applies it; this file
# adds neither. It decides WHAT TO HAND THEM, which is the whole of the problem
# an image poses. Two copies of the rules drift within a week.
#
# THREE TERMS, defined here because this file introduces them (CLAUDE.md's
# terminology discipline):
#
#   authored path      — a file or symlink in the built image that is not
#                        byte-for-byte what the pinned base image has at that
#                        path. Everything the build ADDED or REWROTE, and
#                        nothing it merely inherited.
#   distro floor       — the authored paths the package manager put there. The
#                        runtime layer runs `apk add`, which adds thousands of
#                        files this repo did not write; they are ACCOUNTED FOR
#                        (apk's own ownership ledger says which package owns
#                        each one) rather than exempted by path, because a path
#                        exemption is an allowlist and an allowlist is where a
#                        leak hides.
#   declared binary    — a binary this build authors on purpose. The set is
#                        DERIVED from the Containerfile's `COPY --from=`
#                        destinations, never typed here: the Containerfile is
#                        the one home for what crosses out of the build stage.
#
# HOW AUTHORED CONTENT IS ISOLATED, and why this way. Three mechanisms were
# available: diff the layer digests out of `podman save`, ask
# `podman image diff` for the answer, or export both rootfs and compare them
# here. This file does the third, for two reasons that are not style. It needs
# no JSON parser — `podman image diff --format json` and a save archive's
# manifest both do, and neither `jq` nor `python3` is a dependency of this repo
# — and `podman image diff` does not exist on docker at all, which would make
# the gate silently unavailable on exactly the engine the Makefile falls back
# to. Comparing exported filesystems uses only `create`/`export`, which both
# engines have. It is also the FINER answer: a layer-level diff calls a whole
# layer authored, while this calls a file authored only if its bytes differ
# from the base's at the same path.
#
# UNREADABLE IS REJECTED, NOT SKIPPED — the posture bl-167d fixed in the source
# gate, carried over. The declared binaries are withheld from the table (their
# bytes are `cargo build`'s output over a source tree the source gate reads,
# and `grep -I` cannot read them anyway); ANY OTHER authored file the table
# cannot read reaches `leak-scan.sh`, whose `binary-content` rule refuses it.
#
# BOTH DIRECTIONS. `--self-test` layers a fabricated secret into a file AND
# into an `ENV` on top of the real image and requires the scan to catch each;
# the plain scan of the real image is the other direction. Every enumeration
# this file makes is checked for being non-empty, because a scan that
# enumerates nothing passes everything forever — the same discipline as the
# 300-line cap sweep and `leak-scan --self-test`.
#
# WHAT IT CANNOT PROMISE. It scans one image, on the box that built it, before
# the push. It does not read what is already in the registry, it cannot
# un-publish a digest, and whoever runs the build can bypass it exactly as
# `--no-verify` bypasses the commit hook.

set -euo pipefail

HERE="$(CDPATH= cd -- "$(dirname -- "$(readlink -f "$0" 2>/dev/null || echo "$0")")" && pwd)"
ROOT="$(git rev-parse --show-toplevel)"
cd "$ROOT"

CONTAINERFILE="Containerfile"

# The engine, resolved the way the Makefile resolves it (podman first: no
# daemon, no group membership). `CONTAINER_ENGINE` is the Makefile's own
# variable, so `make image CONTAINER_ENGINE=docker` reaches here unchanged.
ENGINE="${CONTAINER_ENGINE:-$(command -v podman 2>/dev/null || command -v docker 2>/dev/null || true)}"

die() { echo "image-scan: $*" >&2; exit 1; }

SCRATCH="$(mktemp -d "${TMPDIR:-/tmp}/image-scan.XXXXXXXX")"
FIXTURE_TAG=""
cleanup() {
  [ -z "$FIXTURE_TAG" ] || "$ENGINE" rmi -f "$FIXTURE_TAG" >/dev/null 2>&1 || true
  rm -rf "$SCRATCH"
}
trap cleanup EXIT

# --- the two facts read out of the Containerfile ---------------------------

# The runtime base. The LAST `FROM` is the stage that ships; the build stage's
# `FROM` is discarded whole. One home for the pin, exactly as the toolchain
# check inside the Containerfile has one.
base_ref() { awk '/^FROM /{ref=$2} END{print ref}' "$CONTAINERFILE"; }

# The binaries that cross out of the build stage, by their destination path.
expected_binaries() { awk '/^COPY --from=/{print $NF}' "$CONTAINERFILE"; }

# --- mechanism -------------------------------------------------------------

# export_rootfs REF DIR — the image's flattened filesystem on disk.
export_rootfs() {
  local ref="$1" dir="$2" cid
  mkdir -p "$dir"
  cid="$("$ENGINE" create "$ref" 2>/dev/null)" || die "cannot instantiate $ref"
  "$ENGINE" export "$cid" 2>/dev/null | tar -C "$dir" -xf - 2>/dev/null || true
  "$ENGINE" rm -f "$cid" >/dev/null 2>&1 || true
  # An image may ship modes this account cannot read; the scan must not be
  # allowed to pass a file by failing to open it.
  chmod -R u+rwX "$dir" 2>/dev/null || true
}

# authored BUILT BASE — image-absolute paths whose bytes the build changed.
authored() {
  local built="$1" base="$2" rel p
  ( cd "$built" && find . \( -type f -o -type l \) -print ) | LC_ALL=C sort |
  while IFS= read -r rel; do
    p="${rel#.}"
    if [ -L "$built$p" ]; then
      [ -L "$base$p" ] && [ "$(readlink "$built$p")" = "$(readlink "$base$p")" ] && continue
    elif [ -f "$base$p" ] && [ ! -L "$base$p" ] && cmp -s "$built$p" "$base$p"; then
      continue
    fi
    printf '%s\n' "$p"
  done
}

# apk_owned ROOTFS — every path an installed package claims. apk's database is
# the ledger: `F:` sets the directory, `R:` names a file in it.
apk_owned() {
  local db="$1/lib/apk/db/installed"
  [ -f "$db" ] || return 0
  awk -F: '/^F:/{d=substr($0,3); next} /^R:/{if (d != "") print "/" d "/" substr($0,3)}' "$db"
}

# resolve_in_root ROOTFS PATH — follow a symlink chain INSIDE the image, so an
# absolute target means the image's root and not this box's. A chain with `..`
# in it, or a loop, resolves to something the ledger will not match, which
# sends the link to the table — the fail-safe direction.
resolve_in_root() {
  local root="$1" p="$2" n=0 t
  while [ -L "$root$p" ] && [ "$n" -lt 40 ]; do
    t="$(readlink "$root$p")"
    case "$t" in /*) p="$t" ;; *) p="$(dirname "$p")/$t" ;; esac
    n=$((n + 1))
  done
  printf '%s\n' "$p"
}

# config_text IMAGE — the third surface. An `ENV` ships to everyone who pulls
# whether or not any file holds it, and build arguments echo into history.
config_text() {
  "$ENGINE" image inspect "$1" --format '{{range .Config.Env}}Env {{.}}
{{end}}{{range $k, $v := .Config.Labels}}Label {{$k}}={{$v}}
{{end}}Entrypoint {{.Config.Entrypoint}}
Cmd {{.Config.Cmd}}
WorkingDir {{.Config.WorkingDir}}
User {{.Config.User}}
{{range .History}}History {{.CreatedBy}} {{.Comment}}
{{end}}'
}

# --- the scan ---------------------------------------------------------------

# scan_image IMAGE WORK — 0 clean, 1 with findings on stderr.
scan_image() {
  local image="$1" work="$2" base rootfs p r rc=0
  base="$(base_ref)"
  [ -n "$base" ] || die "no FROM line in $CONTAINERFILE"
  mkdir -p "$work"
  rootfs="$work/rootfs"
  export_rootfs "$image" "$rootfs"
  [ -d "$SCRATCH/base" ] || export_rootfs "$base" "$SCRATCH/base"

  authored "$rootfs" "$SCRATCH/base" >"$work/authored.txt"
  [ -s "$work/authored.txt" ] || die "$image has no authored paths above $base — the comparison is broken, not the image"

  apk_owned "$rootfs" | LC_ALL=C sort -u >"$work/owned.txt"
  if [ -f "$rootfs/lib/apk/db/installed" ] && [ ! -s "$work/owned.txt" ]; then
    die "the apk ledger parsed to nothing — the accounting is broken, not the image"
  fi

  expected_binaries | LC_ALL=C sort -u >"$work/declared.txt"
  [ -s "$work/declared.txt" ] || die "no 'COPY --from=' destination in $CONTAINERFILE — the declared binary set cannot be empty"
  while IFS= read -r p; do
    grep -qxF "$p" "$work/authored.txt" || die "declared binary $p is not in $image"
  done <"$work/declared.txt"

  # Partition. Distro floor first: owned by a package, aliased by a symlink
  # into one, or part of the ledger's own storage — a ledger cannot list
  # itself, and one of its members is a tar no text rule can read.
  : >"$work/scan.txt"
  : >"$work/links.txt"
  local distro=0 declared=0
  while IFS= read -r p; do
    case "$p" in /lib/apk/db/*) distro=$((distro + 1)); continue ;; esac
    if grep -qxF "$p" "$work/owned.txt"; then distro=$((distro + 1)); continue; fi
    if [ -L "$rootfs$p" ]; then
      r="$(resolve_in_root "$rootfs" "$p")"
      if grep -qxF "$r" "$work/owned.txt"; then distro=$((distro + 1)); continue; fi
      # A symlink carries no content of its own; its target string is the
      # content, and that is what the table reads.
      printf '%s -> %s\n' "$p" "$(readlink "$rootfs$p")" >>"$work/links.txt"
      continue
    fi
    if grep -qxF "$p" "$work/declared.txt"; then declared=$((declared + 1)); continue; fi
    printf '%s\n' "$rootfs$p" >>"$work/scan.txt"
  done <"$work/authored.txt"

  config_text "$image" >"$work/image-config.txt"
  [ -s "$work/image-config.txt" ] || die "$image reported an empty config — the inspect is broken, not the image"
  printf '%s\n' "$work/image-config.txt" >>"$work/scan.txt"
  [ -s "$work/links.txt" ] && printf '%s\n' "$work/links.txt" >>"$work/scan.txt"

  local files=()
  while IFS= read -r p; do files+=("$p"); done <"$work/scan.txt"
  "$HERE/leak-scan.sh" "${files[@]}" >/dev/null 2>"$work/findings.txt" || rc=1
  if [ "$rc" -ne 0 ]; then
    echo "error: image-scan found material that must not be pushed in $image:" >&2
    # Rewritten to IMAGE paths, so a finding locates a file in the artifact
    # that would be pushed and not one in a scratch directory that is already
    # gone. `leak-scan.sh`'s own banner says "committed"; ours says pushed.
    sed -e '/^error: leak-scan found/d' \
        -e "s#$rootfs##g" -e "s#$work/image-config.txt#<image config>#g" \
        -e "s#$work/links.txt#<image symlinks>#g" "$work/findings.txt" >&2
    return 1
  fi
  echo "image-scan: $image — $(wc -l <"$work/authored.txt" | tr -d ' ') authored paths above ${base%%@*}, $distro on the distro floor, $declared declared binaries, ${#files[@]} scanned by the rule table"
  return 0
}

# shellcheck source=scripts/image-selftest.sh
. "$HERE/image-selftest.sh"

[ -n "$ENGINE" ] || die "no podman and no docker on PATH"
case "${1-}" in
  --self-test) [ -n "${2-}" ] || die "usage: image-scan.sh --self-test IMAGE"; self_test "$2" ;;
  '') die "usage: image-scan.sh IMAGE | image-scan.sh --self-test IMAGE" ;;
  *) scan_image "$1" "$SCRATCH/subject" ;;
esac
