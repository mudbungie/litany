#!/bin/sh
# The regression half of the unattended runs' alarm (bl-2146) —
# `make alarm-selftest`, a step of `lint`.
#
# It drives the REAL `scripts/alarm.sh` under a fake `gh` whose whole world is
# one file of open issues. BOTH DIRECTIONS: an alarm that never opens an issue
# and one that opens one every hour are both broken, so half the cases assert
# exactly one `gh` write with exactly which arguments, and the other half
# assert there was none.
set -eu

HERE=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
SCRIPT="$HERE/alarm.sh"
[ -x "$SCRIPT" ] || { echo "alarm-selftest: $SCRIPT is not executable" >&2; exit 1; }

fails=0
cases=0
OURS='unattended run red: Ingest'
# A near-miss that must never be taken for ours: another workflow's alarm
# whose title merely BEGINS with it. A substring match fails on this line.
DECOY='unattended run red: Ingest nightly'
RUN=https://github.example/o/r/actions/runs/77

# `$open` is the issue file before the run, one `<number><TAB><title>` line
# per open issue. The checker reads `$code`, `$log` (the fake's writes, one
# per line; reads are not logged) and `$after` (the issue file afterwards).
run_case() { # <label> <red|green> <open-issues> <checker>
    cases=$((cases + 1))
    _work=$(mktemp -d "${TMPDIR:-/tmp}/alarm-selftest.XXXXXX")
    mkdir "$_work/bin"
    printf '%s' "$3" > "$_work/issues"
    cat > "$_work/bin/gh" <<FAKE
#!/bin/sh
[ "\$GH_REPO" = o/r ] || { echo "fake gh: GH_REPO is '\$GH_REPO'" >&2; exit 1; }
case "\$1 \$2" in
  'issue list') cat "$_work/issues" ;;
  'issue create')
    [ "\$3" = --title ] && [ "\$5" = --body ] && [ \$# = 6 ] || exit 1
    printf 'create\t%s\t%s\n' "\$4" "\$(printf '%s' "\$6" | tr '\n' ' ')" >> "$_work/log"
    printf '9\t%s\n' "\$4" >> "$_work/issues" ;;
  'issue close')
    [ "\$4" = --comment ] && [ \$# = 5 ] || exit 1
    printf 'close\t%s\t%s\n' "\$3" "\$5" >> "$_work/log"
    grep -v "^\$3	" "$_work/issues" > "$_work/issues.new" || true
    mv "$_work/issues.new" "$_work/issues" ;;
  *) echo "fake gh: unexpected \$*" >&2; exit 1 ;;
esac
FAKE
    chmod 0755 "$_work/bin/gh"
    set +e
    out=$(PATH="$_work/bin:/usr/bin:/bin" GITHUB_REPOSITORY=o/r GITHUB_REPOSITORY_OWNER=o \
        GITHUB_WORKFLOW=Ingest GITHUB_SERVER_URL=https://github.example GITHUB_RUN_ID=77 \
        "$SCRIPT" "$2" 2>&1)
    code=$?
    set -e
    log=$(cat "$_work/log" 2>/dev/null || true)
    after=$(cat "$_work/issues")
    if "$4"; then
        printf '  ok    %s\n' "$1"
    else
        printf '  FAIL  %s (exit %s)\n' "$1" "$code"
        printf '%s\n' "$out" | sed 's/^/          | /'
        printf '%s\n' "${log:-<no gh write>}" | sed 's/^/        gh: /'
        fails=1
    fi
    rm -rf "$_work"
}

# Exactly one create: the exact title, the owner's mention (the notification)
# and the run's link in the body.
created() {
    [ "$code" = 0 ] && [ "$(printf '%s\n' "$log" | grep -c .)" = 1 ] || return 1
    case $log in "create	$OURS	"*"$RUN failed. @o "*'first red run of the streak'*) ;; *) return 1 ;; esac
}
closed() { [ "$code" = 0 ] && [ "$log" = "close	4	$RUN is green" ] && [ "$after" = "5	$DECOY" ]; }
silent() { [ "$code" = 0 ] && [ -z "$log" ]; }
refused() { [ "$code" = 2 ] && [ -z "$log" ]; }

echo 'alarm-selftest: driving alarm.sh under a fake gh'

run_case 'red, none open -> opens exactly one, mentioning the owner' red '' created
run_case 'red, one open -> opens nothing (one notification per streak)' \
    red "4	$OURS
" silent
run_case 'green, one open -> closes it with the run link, and only it' \
    green "5	$DECOY
4	$OURS
" closed
run_case 'green, none open -> does nothing' green '' silent
# The exact-title half, both ways round.
run_case 'red, only a longer-titled alarm open -> still opens ours' \
    red "5	$DECOY
" created
run_case 'green, only a longer-titled alarm open -> closes nothing' \
    green "5	$DECOY
" silent
run_case 'neither red nor green -> refuses, writes nothing' purple '' refused

[ "$cases" -gt 0 ] \
    || { echo 'alarm-selftest: ran 0 cases — the harness is broken' >&2; exit 1; }
[ "$fails" = 0 ] || { echo 'alarm-selftest: the alarm is wrong' >&2; exit 1; }
echo "alarm-selftest: $cases cases, all passed"
