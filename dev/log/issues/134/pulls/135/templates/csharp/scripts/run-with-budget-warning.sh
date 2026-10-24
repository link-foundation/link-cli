#!/usr/bin/env bash
# Run a command under an explicit execution budget that expires *before* the
# job's `timeout-minutes` backstop.
#
# Why this exists: GitHub reports a job killed by `timeout-minutes` as
# `cancelled`, not `failed`. On a non-default ref that is indistinguishable
# from a superseded run, so a genuine timeout produces no red anywhere. The
# fix is to make the step own the deadline: when the step's budget expires
# first, the step fails, the job fails, and the annotation names the budget and
# the overrun.
#
# Usage:
#   scripts/run-with-budget-warning.sh <budget_seconds> <label> <command> [args...]
#
# Environment:
#   BUDGET_WARN_PERCENT  - emit a ::warning at this share of the budget (default 70)
#   BUDGET_GRACE_SECONDS - seconds between SIGTERM and SIGKILL (default 10)
#   BUDGET_POLL_SECONDS  - poll interval; may be fractional (default 1)
#
# Exit codes:
#   124 - the command exceeded its budget (matches timeout(1)'s convention)
#     2 - usage error
#     * - the command's own exit status
set -uo pipefail

usage() {
  echo "usage: $0 <budget_seconds> <label> <command> [args...]" >&2
  exit 2
}

[ "$#" -ge 3 ] || usage

BUDGET="$1"
LABEL="$2"
shift 2

case "$BUDGET" in
  '' | *[!0-9]*) echo "budget must be a positive integer number of seconds, got: ${BUDGET}" >&2; usage ;;
esac
[ "$BUDGET" -gt 0 ] || usage

# Every knob below lands in bash arithmetic (`$(( ... ))` / `[ -lt ... ]`), which
# aborts on a non-integer and -- under this script's non-`set -e` mode -- would
# leave the loop running with a frozen elapsed value: the budget would never
# expire and the job cap would fire first, exactly the failure this script
# exists to prevent. Validate all of them before the loop starts.
case "${BUDGET_WARN_PERCENT:-70}" in
  '' | *[!0-9]*)
    echo "BUDGET_WARN_PERCENT must be an integer percentage, got: ${BUDGET_WARN_PERCENT:-<unset>}" >&2
    usage
    ;;
esac
WARN_PERCENT="${BUDGET_WARN_PERCENT:-70}"

case "${BUDGET_GRACE_SECONDS:-10}" in
  '' | *[!0-9]*)
    echo "BUDGET_GRACE_SECONDS must be a non-negative integer number of seconds, got: ${BUDGET_GRACE_SECONDS:-<unset>}" >&2
    usage
    ;;
esac
GRACE_SECONDS="${BUDGET_GRACE_SECONDS:-10}"

# The poll interval may be fractional (0.5, 0.25) because `sleep` accepts
# decimals -- but it must still be a plain decimal number greater than zero.
case "${BUDGET_POLL_SECONDS:-1}" in
  '' | *[!0-9.]* | *.*.* | . | .* | *.)
    echo "BUDGET_POLL_SECONDS must be a positive number of seconds (e.g. 1, 0.5), got: ${BUDGET_POLL_SECONDS:-<unset>}" >&2
    usage
    ;;
esac
POLL_SECONDS="${BUDGET_POLL_SECONDS:-1}"
case "$POLL_SECONDS" in
  *[1-9]*) ;;
  *) echo "BUDGET_POLL_SECONDS must be greater than zero, got: ${POLL_SECONDS}" >&2; usage ;;
esac

warn_at=$(( BUDGET * WARN_PERCENT / 100 ))

# `set -m` gives the background command its own process group. Without it,
# `timeout(1)` or a plain `kill $pid` reaches only the direct child, while
# `cargo test` / `cargo nextest` spawn a tree whose orphans keep the runner
# busy until the job cap fires -- exactly the failure this script prevents.
set -m
"$@" &
command_pid=$!
set +m

# Signal the whole process group when possible, and fall back to the direct
# child on platforms where the group is not addressable (Git Bash on Windows).
signal_tree() {
  kill "-$1" -- "-${command_pid}" 2>/dev/null || kill "-$1" "${command_pid}" 2>/dev/null || true
}

forward() {
  signal_tree TERM
  exit 143
}
trap forward TERM INT

# Elapsed time comes from the shell's own $SECONDS clock, not from counting
# poll iterations: a fractional or variable poll interval must not change when
# the budget expires, and it cannot make the count drift from real time.
started=$SECONDS
warned=false
timed_out=false
while kill -0 "${command_pid}" 2>/dev/null; do
  elapsed=$(( SECONDS - started ))
  if [ "${elapsed}" -ge "${BUDGET}" ]; then
    timed_out=true
    break
  fi
  if [ "${warned}" = false ] && [ "${elapsed}" -ge "${warn_at}" ]; then
    warned=true
    echo "::warning title=${LABEL} is approaching its execution budget::${LABEL} has been running for ${elapsed}s of its ${BUDGET}s budget (${WARN_PERCENT}%). If it keeps growing, raise the budget and the job's timeout-minutes together, or split the work."
  fi
  sleep "${POLL_SECONDS}"
done

if [ "${timed_out}" = true ]; then
  measured=$(( SECONDS - started ))
  signal_tree TERM
  waited=0
  while [ "${waited}" -lt "${GRACE_SECONDS}" ] && kill -0 "${command_pid}" 2>/dev/null; do
    sleep 1
    waited=$(( waited + 1 ))
  done
  signal_tree KILL
  wait "${command_pid}" 2>/dev/null || true
  echo "::error title=${LABEL} exceeded its execution budget::${LABEL} ran for ${measured}s against its ${BUDGET}s budget (detected at ${POLL_SECONDS}s poll granularity) and was terminated. This budget expires before the job's timeout-minutes backstop on purpose: a job killed by timeout-minutes is reported as 'cancelled' and hides the failure, while this reports 'failure'. Command: $*"
  exit 124
fi

status=0
wait "${command_pid}" || status=$?
exit "${status}"
