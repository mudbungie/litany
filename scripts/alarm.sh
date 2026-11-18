#!/bin/sh
# The unattended runs' alarm (bl-2146): `scripts/alarm.sh red|green`, the last
# step of every scheduled workflow. `scripts/alarm-selftest.sh` drives THIS
# file under a fake `gh` and is what `make alarm-selftest` runs.
#
# **Why.** A scheduled run that fails tells nobody: GitHub mails the person
# who last edited the cron, at best, and an hourly workflow that did mail
# would mail every hour until somebody muted it. So a red streak is ONE
# notification — an issue that mentions the repository owner, which is what
# makes GitHub send it — and the next green run closes that issue.
#
# **Nothing is stored.** "Is this workflow in a red streak" is the open issue
# whose title is exactly the one below, asked of GitHub each time. Close it by
# hand and the next red run opens another, which is the right answer.
#
# No label is applied: `gh issue create` fails on a label the repository does
# not have, and an alarm that fails on a fresh repository is no alarm.
set -eu

export GH_REPO="$GITHUB_REPOSITORY"
title="unattended run red: $GITHUB_WORKFLOW"
run="$GITHUB_SERVER_URL/$GITHUB_REPOSITORY/actions/runs/$GITHUB_RUN_ID"

# The open issue with exactly that title, or nothing. The match is made here,
# on whole fields, rather than by a search: GitHub's search is a substring
# match, and a workflow whose name begins with another's would share its alarm.
listing=$(gh issue list --state open --limit 1000 --json number,title \
    --jq '.[] | "\(.number)\t\(.title)"')
open=$(printf '%s\n' "$listing" | awk -F'\t' -v t="$title" '$2 == t { print $1; exit }')

case ${1:-} in
    red)
        # One open already: this streak has been announced. Saying nothing is
        # the half that keeps an hourly failure from being an hourly email.
        [ -z "$open" ] || { echo "alarm: #$open is already open for this streak"; exit 0; }
        gh issue create --title "$title" --body "$run failed. @$GITHUB_REPOSITORY_OWNER

This is the first red run of the streak. Later red runs of this workflow add nothing here while this issue is open, and the next green run closes it."
        ;;
    green)
        [ -n "$open" ] || exit 0
        gh issue close "$open" --comment "$run is green"
        ;;
    *)
        echo 'usage: alarm.sh red|green' >&2
        exit 2
        ;;
esac
