#!/bin/sh
# The regression half of the standalone reconciler (bl-8cfd) —
# `make deploy-selftest`, a step of `lint`.
#
# **It drives the REAL `litany-update`**, end to end, under a fake `curl` and a
# fake `cargo` on `PATH` and a scratch `HOME`. Nothing here re-implements the
# decision: a self-test that restated the rule would prove only that the copy
# still agrees with itself, which is the failure `leak-scan --self-test` is
# written to avoid. Every assertion below is about what the shipped file did.
#
# It touches no machine and needs no network, no registry, no toolchain and no
# release, which is what lets it run in the gate — and that is the point. A
# reconciler is unattended code on somebody's workstation, so the failures that
# matter are the quiet ones: it stops installing, or it starts installing the
# wrong thing, and nobody finds out for a month.
#
# BOTH DIRECTIONS, and that is the shape of the table rather than a footnote.
# Half the cases assert an install HAPPENED and with exactly which arguments;
# the other half assert `cargo` was never invoked at all. A reconciler that
# installs on every tick and one that has quietly stopped are both broken, and
# only one of them is loud.
set -eu

HERE=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
SCRIPT="$HERE/litany-update"
[ -x "$SCRIPT" ] || { echo "deploy-selftest: $SCRIPT is not executable" >&2; exit 1; }

fails=0
cases=0

# The synthetic index. Real sparse-index lines carry a full `deps` array and a
# checksum; the reconciler reads `"vers"` and `"yanked"` by parameter expansion
# and nothing else, so the fixture keeps the one field a greedy match could run
# past — a dependency's `"req"` — and omits the rest.
line() { # <version> <yanked:true|false>
    printf '{"name":"litany","vers":"%s","deps":[{"name":"brazen","req":"=0.0.17"}],' "$1"
    printf '"features":{},"yanked":%s}\n' "$2"
}

current()    { line 0.0.10 false; line 0.0.11 false; }
tip_yanked() { line 0.0.10 false; line 0.0.11 true;  }
all_yanked() { line 0.0.10 true;  line 0.0.11 true;  }

# Run the reconciler in a built world and hand the result to a checker, which
# reads four globals: `$code` its exit status, `$out` its combined output,
# `$log` the fake cargo's recorded argv (empty when it was never called), and
# `$root` the litany install root that world's `$HOME` implies.
#
# A world is described by the two versions on its box — the litany on disk and
# the `bz` on disk — because those two ARE the state the reconciler reads. The
# fake litany prints the `litany <v> (brazen <pin>)` line the real one prints,
# and the pin it states is what the adapter arm must obey.
run_case() { # <label> <index-fn> <litany-version> <litany-pin> <bz-version> <checker>
    _label=$1 _fixture=$2 _have=$3 _pin=$4 _bz=$5 _check=$6
    cases=$((cases + 1))
    _work=$(mktemp -d "${TMPDIR:-/tmp}/litany-selftest.XXXXXX")
    mkdir -p "$_work/bin" "$_work/home/.local/bin"
    "$_fixture" > "$_work/index.txt"
    root="$_work/home/.local"

    # `curl`: serves the fixture for the sparse-index URL and refuses anything
    # else with curl's own exit 22, so a reconciler that started fetching a
    # second thing fails here rather than passing silently.
    cat > "$_work/bin/curl" <<EOF
#!/bin/sh
for a in "\$@"; do :; done
[ "\$a" = https://index.crates.io/li/ta/litany ] \
    || { echo "fake curl: unexpected URL \$a" >&2; exit 22; }
exec cat "$_work/index.txt"
EOF
    # `cargo`: records its whole argv, then emulates the install by writing the
    # binary the reconciler re-reads afterwards. Emulating it is what makes the
    # final report lines assertions rather than hopes. A litany install writes
    # the two-part version line, carrying the pin the fixture named.
    cat > "$_work/bin/cargo" <<EOF
#!/bin/sh
printf '%s\n' "\$*" >> "$_work/cargo.log"
_crate=\$2; _root=; _vers=
while [ \$# -gt 0 ]; do
  case \$1 in --root) _root=\$2; shift ;; --version) _vers=\$2; shift ;; esac
  shift
done
if [ "\$_crate" = brazen ]; then
  mkdir -p "$_work/bin"
  printf '#!/bin/sh\necho "bz %s"\n' "\$_vers" > "$_work/bin/bz"
  chmod 0755 "$_work/bin/bz"
else
  mkdir -p "\$_root/bin"
  printf '#!/bin/sh\necho "litany %s (brazen $_pin)"\n' "\$_vers" > "\$_root/bin/litany"
  chmod 0755 "\$_root/bin/litany"
fi
EOF
    chmod 0755 "$_work/bin/curl" "$_work/bin/cargo"

    # A litany with no pin is not an empty pin: it is a litany too old to
    # print the parenthesised half at all, which is the shape the adapter arm
    # has to refuse.
    if [ -n "$_have" ]; then
        if [ -n "$_pin" ]; then
            printf '#!/bin/sh\necho "litany %s (brazen %s)"\n' "$_have" "$_pin" \
                > "$root/bin/litany"
        else
            printf '#!/bin/sh\necho "litany %s"\n' "$_have" > "$root/bin/litany"
        fi
        chmod 0755 "$root/bin/litany"
    fi
    if [ -n "$_bz" ]; then
        printf '#!/bin/sh\necho "bz %s"\n' "$_bz" > "$_work/bin/bz"
        chmod 0755 "$_work/bin/bz"
    fi

    set +e
    out=$(HOME="$_work/home" PATH="$_work/bin:/usr/bin:/bin" "$SCRIPT" 2>&1)
    code=$?
    set -e
    log=$(cat "$_work/cargo.log" 2>/dev/null || true)

    if "$_check"; then
        printf '  ok    %s\n' "$_label"
    else
        printf '  FAIL  %s (exit %s)\n' "$_label" "$code"
        printf '%s\n' "$out" | sed 's/^/          | /'
        printf '        cargo: %s\n' "${log:-<never invoked>}"
        fails=1
    fi
    rm -rf "$_work"
}

# The exact argument vectors, not merely "cargo ran". `--version` with
# `--force` IS the yank lever's mechanism — without both, cargo refuses to move
# backwards and a rollback silently does nothing — and `--root` is what keeps
# the new litany on the path `make install` writes, so the box never holds two.
# `bz` takes no `--root`, matching `make install-bz`, for that same reason.
litany_vector() { # <version>
    printf '%s\n' "$log" | grep -qx "install litany --locked --version $1 --force --root $root"
}
bz_vector() { # <version>
    printf '%s\n' "$log" | grep -qx "install brazen --locked --version $1 --force"
}
said() { printf '%s\n' "$out" | grep -q "$1"; }

upgraded_and_pinned() {
    [ "$code" = 0 ] && litany_vector 0.0.11 && bz_vector 0.0.17 \
        && said 'installing 0\.0\.11 (was 0\.0\.4)'
}
bootstrapped() {
    [ "$code" = 0 ] && litany_vector 0.0.11 && bz_vector 0.0.17 \
        && said 'installing 0\.0\.11 (was absent)' \
        && said 'installing brazen 0\.0\.17 (was absent)'
}
# The negative arm, and it is an assertion about a NON-event: the recorder file
# was never written, so nothing was installed.
fully_current() {
    [ "$code" = 0 ] && [ -z "$log" ] && said 'installed 0\.0\.11 is current' \
        && said 'installed bz 0\.0\.17 matches the pin'
}
# The sighting this ball was filed from, in one case: an unchanged litany
# beside a `bz` that does not match its pin. The litany arm must do nothing and
# the adapter arm must still fire — and it must fire DOWNWARDS, since a newer
# `bz` is as refused by the load-time guard as an older one.
adapter_only_downgrade() {
    [ "$code" = 0 ] && bz_vector 0.0.6 && said 'installed 0\.0\.11 is current' \
        && ! printf '%s\n' "$log" | grep -q '^install litany '
}
rolled_back() {
    [ "$code" = 0 ] && litany_vector 0.0.10 && said 'installing 0\.0\.10 (was 0\.0\.11)'
}
refused_with() { # <message-pattern>
    [ "$code" != 0 ] && [ -z "$log" ] && said "$1"
}
refused_empty()   { refused_with 'named no live version'; }
refused_offline() { refused_with 'cannot reach the registry index'; }
refused_no_pin()  { [ "$code" != 0 ] && said 'does not state its brazen pin'; }

echo 'deploy-selftest: driving litany-update under fake curl/cargo'

# Behind the registry: it installs the newest live litany, then the `bz` that
# litany names. Both arms, one tick.
run_case 'behind the registry -> installs litany, then its pinned bz' \
    current 0.0.4 0.0.17 0.0.6 upgraded_and_pinned
# A box with neither binary is that same question with empty answers rather
# than a special case: both compares differ, so bootstrap and upgrade are one
# path.
run_case 'neither binary present -> installs both, reporting "was absent"' \
    current '' 0.0.17 '' bootstrapped
# The negative arm. Nothing to do must mean nothing done — on BOTH arms.
run_case 'litany current and bz matching -> does NOT invoke cargo' \
    current 0.0.11 0.0.17 0.0.17 fully_current
# The live sighting, in one case: the adapter arm firing on a tick where litany
# did NOT move. This is the state the ball was filed from — a `bz` on the box
# that its litany's guard refuses — and here the pin is OLDER than the `bz`
# installed, because a newer adapter is as refused as an older one, so the fix
# is a downgrade.
run_case 'litany unchanged but bz mismatched -> moves bz to the pin, downwards' \
    current 0.0.11 0.0.6 0.0.17 adapter_only_downgrade
# The rollback lever: a yank makes the previous version newest-live, the
# compare sees it differ from what is installed, and the install goes BACKWARDS.
run_case 'newest yanked -> rolls the box back a version' \
    tip_yanked 0.0.11 0.0.17 0.0.17 rolled_back
# Nothing live at all, and no litany on the box to fall back on: there is no
# version to install and none to read a pin from.
run_case 'nothing live and no binary -> refuses, installs nothing' \
    all_yanked '' 0.0.17 '' refused_empty
# A litany too old to state its pin. The adapter arm has no answer to obey, and
# guessing one is how a box gets a `bz` its litany refuses.
run_case 'installed litany states no pin -> refuses rather than guessing' \
    current 0.0.11 '' 0.0.17 refused_no_pin

# The registry unreachable. A refusing `curl` and an EMPTY body are different
# failures and must not report as one, so this drives the first: the shim exits
# 22 the way curl does, and the reconciler must name the registry rather than
# the index's contents. The fixture is the shim itself, so it takes no case
# through `run_case`.
cases=$((cases + 1))
work=$(mktemp -d "${TMPDIR:-/tmp}/litany-selftest.XXXXXX")
mkdir -p "$work/bin" "$work/home"
printf '#!/bin/sh\nexit 22\n' > "$work/bin/curl"
printf '#!/bin/sh\nprintf "%%s\\n" "$*" >> "%s/cargo.log"\n' "$work" > "$work/bin/cargo"
chmod 0755 "$work/bin/curl" "$work/bin/cargo"
set +e
out=$(HOME="$work/home" PATH="$work/bin:/usr/bin:/bin" "$SCRIPT" 2>&1); code=$?
set -e
log=$(cat "$work/cargo.log" 2>/dev/null || true)
if refused_offline; then
    printf '  ok    %s\n' 'registry unreachable -> refuses, installs nothing'
else
    printf '  FAIL  %s (exit %s)\n' 'registry unreachable -> refuses, installs nothing' "$code"
    printf '%s\n' "$out" | sed 's/^/          | /'
    fails=1
fi
rm -rf "$work"

# The empty-set guard, the same two-direction discipline `leak-scan
# --self-test` holds: a table that ran no case is a broken harness, not a clean
# reconciler, and it must not pass as green.
[ "$cases" -gt 0 ] \
    || { echo 'deploy-selftest: ran 0 cases — the harness is broken' >&2; exit 1; }
[ "$fails" = 0 ] || { echo 'deploy-selftest: the reconciler is wrong' >&2; exit 1; }
echo "deploy-selftest: $cases cases, all passed"
