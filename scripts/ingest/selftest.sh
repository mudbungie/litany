#!/bin/sh
# The regression half of the brazen ingest (bl-5866) — `make ingest-selftest`,
# a step of `lint`.
#
# **It drives the REAL `make ingest`** — this repository's Makefile and
# `scripts/ingest/ingest`, copied into a scratch git repository — under a fake
# `curl` and a fake `cargo` on `PATH`. Nothing here re-implements the decision
# (`scripts/deploy/update-selftest.sh` says why that matters); every assertion
# is about what the shipped files did to a tree.
#
# BOTH DIRECTIONS. Half the cases assert the tree MOVED and exactly how; the
# other half assert `git status` is empty afterwards. An ingest that lands a
# commit every hour and one that has quietly stopped are both broken, and only
# one of them is loud.
set -eu

HERE=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
REPO=$(CDPATH= cd -- "$HERE/../.." && pwd)
[ -x "$HERE/ingest" ] || { echo "ingest-selftest: $HERE/ingest is not executable" >&2; exit 1; }

# Hermetic git and make: the gate runs this inside a hook, which exports the
# hook's own repository and index, and inside a `make`, which exports its flags.
unset GIT_DIR GIT_WORK_TREE GIT_INDEX_FILE MAKEFLAGS MFLAGS MAKELEVEL
export GIT_CONFIG_GLOBAL=/dev/null GIT_CONFIG_NOSYSTEM=1
export GIT_AUTHOR_NAME=t GIT_AUTHOR_EMAIL=t@example.invalid
export GIT_COMMITTER_NAME=t GIT_COMMITTER_EMAIL=t@example.invalid

URL=https://github.com/mudbungie/litany/compare
# The tag the world's release commit carries. One case moves it off the
# manifest's version, which is what a release in flight looks like.
tag=litany-v0.0.15
fails=0
cases=0

line() { # <version> <yanked:true|false>
    printf '{"name":"brazen","vers":"%s","deps":[{"name":"libc","req":"^0.2"}],' "$1"
    printf '"features":{},"yanked":%s}\n' "$2"
}
newer()      { line 0.0.20 false; line 0.0.21 false; line 0.0.22 false; }
current()    { line 0.0.20 false; line 0.0.21 false; }
tip_yanked() { line 0.0.21 false; line 0.0.22 true; }
pin_yanked() { line 0.0.20 false; line 0.0.21 true; }
prerelease() { line 0.0.21 false; line 0.1.0-rc1 false; }

# Three changelogs, by what is waiting above the last release (0.0.15, which
# carries the 0.0.21 pin's own bullet): nothing; a delivery's bullet under
# [Unreleased]; and that bullet already promoted to a 0.0.16 that has not
# released.
released() {
    printf '## [0.0.15](%s/litany-v0.0.14...litany-v0.0.15) - 2026-01-01\n\n' "$URL"
    printf -- '- Pin brazen `=0.0.21` (a hand wrote this one). [bl-0000]\n'
}
settled()  { printf '# Changelog\n\n## [Unreleased]\n\n'; released; }
waiting()  { printf '# Changelog\n\n## [Unreleased]\n\n- A waiting thing. [bl-1111]\n\n'; released; }
promoted() {
    printf '# Changelog\n\n## [Unreleased]\n\n'
    printf '## [0.0.16](%s/litany-v0.0.15...litany-v0.0.16) - 2026-01-02\n\n' "$URL"
    printf -- '- A waiting thing. [bl-1111]\n\n'
    released
}

# Build a world, tick it, hand the result to a checker. The checker reads
# `$w` (the tree), `$code`/`$out` (the run), `$log` (the fake cargo's argv)
# and `$dirt` (`git status --porcelain` afterwards — empty means nothing moved).
run_case() { # <label> <index-fn> <changelog-fn> <window-commit-subject> <checker>
    _label=$1 _check=$5
    cases=$((cases + 1))
    _work=$(mktemp -d "${TMPDIR:-/tmp}/ingest-selftest.XXXXXX")
    w=$_work/tree
    mkdir -p "$_work/bin" "$w/scripts/ingest"
    cp "$REPO/Makefile" "$w/Makefile"
    cp "$HERE/ingest" "$w/scripts/ingest/ingest"
    "$3" > "$w/CHANGELOG.md"
    printf '[workspace.dependencies]\nbrazen = "=0.0.21"\n\n[package]\nname = "litany"\nversion = "0.0.15"\n' \
        > "$w/Cargo.toml"
    printf 'name = "brazen"\nversion = "0.0.21"\n' > "$w/Cargo.lock"
    {
        echo 'cargo install brazen --version =0.0.21 --locked'
        echo 'prints `litany <version> (brazen 0.0.21)`; the home is `brazen = "=<pin>"`'
        echo 'links (`brazen = "=0.0.21"`) for the types'
    } > "$w/README.md"
    git -C "$w" init -q -b main
    git -C "$w" add -A
    git -C "$w" commit -qm 'the last release'
    git -C "$w" tag "$tag"
    git -C "$w" commit -q --allow-empty -m "$4"

    # `curl` serves the fixture for the one URL and refuses any other with
    # curl's own exit 22; an index function that does not exist is the
    # unreachable registry. `cargo` records its argv and moves the lockfile,
    # so "nothing moved" is a fact about the whole tree.
    if "$2" > "$_work/index.txt" 2>/dev/null; then
        cat > "$_work/bin/curl" <<EOF
#!/bin/sh
for a in "\$@"; do :; done
[ "\$a" = https://index.crates.io/br/az/brazen ] \
    || { echo "fake curl: unexpected URL \$a" >&2; exit 22; }
exec cat "$_work/index.txt"
EOF
    else
        printf '#!/bin/sh\nexit 22\n' > "$_work/bin/curl"
    fi
    cat > "$_work/bin/cargo" <<EOF
#!/bin/sh
printf '%s\n' "\$*" >> "$_work/cargo.log"
printf 'name = "brazen"\nversion = "%s"\n' "\$5" > Cargo.lock
EOF
    chmod 0755 "$_work/bin/curl" "$_work/bin/cargo"

    set +e
    out=$(cd "$w" && PATH="$_work/bin:/usr/bin:/bin" make --no-print-directory ingest 2>&1)
    code=$?
    set -e
    log=$(cat "$_work/cargo.log" 2>/dev/null || true)
    dirt=$(git -C "$w" status --porcelain)

    if "$_check"; then
        printf '  ok    %s\n' "$_label"
    else
        printf '  FAIL  %s (exit %s)\n' "$_label" "$code"
        printf '%s\n' "$out" | sed 's/^/          | /'
        printf '        cargo: %s\n' "${log:-<never invoked>}"
        printf '%s\n' "$dirt" | sed 's/^/        dirt: /'
        sed 's/^/        changelog: /' "$w/CHANGELOG.md"
        fails=1
    fi
    rm -rf "$_work"
}

has()   { grep -qF -- "$2" "$w/$1"; }
count() { grep -cF -- "$2" "$w/$1" || true; }
# The line numbers of a changelog line, so ORDER is assertable: a bullet is
# under a heading only if it comes after it and before the next one.
at()    { grep -nF -- "$1" "$w/CHANGELOG.md" | head -1 | cut -d: -f1; }

# The pin's one home, all three README shapes, the placeholder left alone, the
# lockfile moved by exactly this argv, and a bullet under a heading for $1.
moved_to() { # <next litany version>
    [ "$code" = 0 ] || return 1
    has Cargo.toml 'brazen = "=0.0.22"' && has Cargo.lock 'version = "0.0.22"' \
        && [ "$log" = 'update -p brazen --precise 0.0.22' ] \
        && [ "$(count README.md 0.0.22)" = 3 ] && [ "$(count README.md 0.0.21)" = 0 ] \
        && has README.md '"=<pin>"' \
        && [ "$(count CHANGELOG.md "## [$1]($URL/litany-v0.0.15...litany-v$1) - ")" = 1 ] \
        && [ "$(at '## [Unreleased]')" -lt "$(at "## [$1](")" ] \
        && [ "$(at "## [$1](")" -lt "$(at '- Pin brazen `=0.0.22`')" ] \
        && [ "$(at '- Pin brazen `=0.0.22`')" -lt "$(at '## [0.0.15](')" ]
}
moved_patch() { moved_to 0.0.16; }
moved_minor() { moved_to 0.1.0; }
# Whatever was waiting ships under the SAME heading as the pin, and a 0.0.16
# that was already stamped is not stamped twice (`moved_to` counts one).
moved_with_the_waiting() {
    moved_to 0.0.16 \
        && [ "$(at '## [0.0.16](')" -lt "$(at '- A waiting thing.')" ] \
        && [ "$(at '- A waiting thing.')" -lt "$(at '## [0.0.15](')" ]
}
# The negative arm: a NON-event, asserted on the tree and not on the output.
untouched() { [ "$code" = 0 ] && [ -z "$dirt" ]; }
# The hold: green, nothing written, and it stopped BEFORE the registry and
# cargo — a hold that had already rewritten the pin would land half a move.
held_in_flight() {
    untouched && [ -z "$log" ] \
        && printf '%s\n' "$out" | grep -q 'no tag litany-v0.0.15: a release is still in flight'
}
refused_offline() {
    [ "$code" != 0 ] && [ -z "$dirt" ] && [ -z "$log" ] \
        && printf '%s\n' "$out" | grep -q 'cannot reach the registry index'
}

echo 'ingest-selftest: driving make ingest under fake curl/cargo'

run_case 'a newer brazen -> pin, README, lockfile and a promoted bullet all move' \
    newer settled 'a delivery [bl-2222]' moved_patch
run_case 'nothing newer -> the same run leaves the tree untouched' \
    current settled 'a delivery [bl-2222]' untouched
# A yanked newest is not a release. Without the filter this would move.
run_case 'newest is yanked -> skipped, tree untouched' \
    tip_yanked settled 'a delivery [bl-2222]' untouched
# The pinned version itself yanked: newest live is OLDER than the pin.
run_case 'newest live is older than the pin -> never downgrades' \
    pin_yanked settled 'a delivery [bl-2222]' untouched
run_case 'newest is a pre-release -> skipped, tree untouched' \
    prerelease settled 'a delivery [bl-2222]' untouched
run_case 'bullets waiting under [Unreleased] -> they ship under the pin'"'"'s heading' \
    newer waiting 'a delivery [bl-1111]' moved_with_the_waiting
run_case 'a promotion that has not released -> re-stamped once, not twice' \
    newer promoted 'a delivery [bl-1111]' moved_with_the_waiting
# The number must be the one release-plz will propose, or the hold never clears.
run_case 'a [minor] marker since the last tag -> promoted to the minor' \
    newer settled 'a bigger delivery [minor] [bl-3333]' moved_minor
run_case 'registry unreachable -> refuses, tree untouched, cargo never run' \
    unreachable settled 'a delivery [bl-2222]' refused_offline
# Main's version has no tag yet although a newer brazen is waiting: green,
# and nothing moves until the tag exists.
tag=litany-v0.0.14
run_case 'release in flight (no tag for main'"'"'s version) -> holds green, tree untouched' \
    newer settled 'chore: release v0.0.15' held_in_flight
tag=litany-v0.0.15

[ "$cases" -gt 0 ] \
    || { echo 'ingest-selftest: ran 0 cases — the harness is broken' >&2; exit 1; }
[ "$fails" = 0 ] || { echo 'ingest-selftest: the ingest is wrong' >&2; exit 1; }
echo "ingest-selftest: $cases cases, all passed"
