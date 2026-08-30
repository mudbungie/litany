#!/usr/bin/env bash
# The regression half of `image-scan.sh`, and the point of it. Sourced by the
# scanner so it exercises the same functions the gate runs — a self-test that
# reimplemented the walk would prove the reimplementation.
#
# A disclosure gate does not die by being wrong. It dies by silently matching
# nothing after something is edited, and then passing every image forever. So
# this plants fabricated secrets ON TOP OF THE REAL IMAGE, in each of the two
# places an image can carry one, and requires the scan to name all three
# findings:
#
#   a layer file      — a file the build added above the base. This is the
#                       whole authored-path walk, the distro-floor accounting
#                       and the rule table, end to end.
#   an ENV            — the config surface, which no filesystem walk can see
#                       and which ships to everyone who pulls.
#   an unreadable file— a binary the build authored that is not one of the
#                       Containerfile's declared `COPY --from=` destinations.
#                       This is the "unreadable is rejected, not skipped"
#                       posture; without it the one class of file most likely
#                       to carry a dump passes by being unopenable.
#
# Every planted value carries `notreal`, the same marker `leak-rules.sh`
# requires of its own fixtures and for the same reason: no regex can tell a
# real secret from a fabricated one, and only the value can say so.
#
# The other direction — the real image passing clean — is the plain scan, and
# `make image-scan` runs both.

# assert_finding LABEL NEEDLE FILE — one beat, and it emits a row on BOTH
# outcomes. A beat that speaks only when it succeeds deletes itself from the
# verdict on the one failure it exists to catch.
assert_finding() {
  if grep -qF -- "$2" "$3"; then
    echo "  self-test: caught $1"
    return 0
  fi
  echo "  self-test: MISSED $1 — no finding matching '$2'" >&2
  return 1
}

self_test() {
  local image="$1" dir="$SCRATCH/fixture" out="$SCRATCH/selftest.txt" fails=0 rc=0

  # THE TWO PLANTED VALUES ARE SPLIT ACROSS A STRING JOIN, and that is not
  # decoration. This file is a tracked file of a repo whose commit gate is the
  # very table being tested, so a fabricated token written whole here would
  # fail `make leak-scan` forever. It is the same idiom `leak-rules.sh` uses on
  # the one pattern that matched its own text (`Fil[e]`, the `[s]shd` trick):
  # the value the fixture SHIPS is real-shaped, the value this file HOLDS is
  # not. Both carry `notreal` for the reason every rule fixture does — no regex
  # can tell a fabricated secret from a live one, and only the value can say so.
  local aws="AKIA""NOTREAL0NOTREAL1" ant="sk-ant-""api03-notrealnotrealnotreal"

  mkdir -p "$dir"
  {
    echo "a fabricated AWS-shaped key, notreal, planted by image-selftest.sh:"
    echo "$aws"
  } >"$dir/notes.txt"
  printf 'notreal\000\001\002 unreadable fixture\n' >"$dir/blob.bin"
  cat >"$dir/Containerfile" <<EOF
FROM $image
COPY notes.txt /srv/notreal-notes.txt
COPY blob.bin /srv/notreal-blob.bin
ENV NOTREAL_TOKEN=$ant
EOF

  FIXTURE_TAG="localhost/image-scan-selftest:notreal"
  "$ENGINE" build -q -f "$dir/Containerfile" -t "$FIXTURE_TAG" "$dir" >/dev/null 2>&1 ||
    die "self-test could not build the fixture image on top of $image"

  scan_image "$FIXTURE_TAG" "$SCRATCH/fixture-scan" 2>"$out" >/dev/null || rc=1
  [ "$rc" -eq 1 ] || {
    echo "self-test: the fixture image PASSED — the scan is broken, not the image" >&2
    exit 1
  }

  assert_finding "a fabricated token in a layer file" \
    "/srv/notreal-notes.txt" "$out" || fails=1
  assert_finding "a fabricated token planted in ENV" \
    "<image config>" "$out" || fails=1
  assert_finding "an undeclared unreadable file the build authored" \
    "/srv/notreal-blob.bin" "$out" || fails=1
  # Each finding must also carry the rule that named it, or a single
  # over-broad rule could be answering for all three.
  assert_finding "the vendor-token rule" "[vendor-token]" "$out" || fails=1
  assert_finding "the binary-content rule" "[binary-content]" "$out" || fails=1

  [ "$fails" -eq 0 ] || exit 1
  echo "image-scan: self-test OK — a layer secret, an ENV secret and an undeclared binary are all caught"
}
