#!/usr/bin/env bash
# Terminal status gate: observe every job of the workflow and turn a hidden
# timeout into a visible failure.
#
# GitHub reports jobs killed by `timeout-minutes` as `cancelled`. On a
# non-default ref that is indistinguishable from a superseded run; on the
# default branch it would otherwise vanish into a grey run.
#
# Environment:
#   NEEDS_JSON  - required; `${{ toJSON(needs) }}` from the gate job
#   IS_MAIN     - 'true' when this run is a push to the default branch
#   RUN_SHA     - the commit this run was started for (${{ github.sha }})
#   BRANCH_REF  - the branch name the run is on (${{ github.ref_name }})
#   BRANCH_HEAD_SHA - optional pre-resolved head of BRANCH_REF; when unset the
#                     head is resolved with `git ls-remote origin`
#
# A cancelled job on main is a failure only when this run was still the head of
# the branch: a run whose commit has since been superseded was cancelled by
# GitHub's concurrency handling, which is expected churn. When the head cannot
# be resolved at all (missing env, unreachable remote), the run is treated as
# NOT superseded and the check fails loud -- a silent pass here would be the
# exact bug this gate exists to prevent (issue #156).
set -euo pipefail

: "${NEEDS_JSON:?NEEDS_JSON is required (pass toJSON(needs))}"
IS_MAIN="${IS_MAIN:-false}"

select_by_result() {
  printf '%s' "$NEEDS_JSON" \
    | python3 -c 'import json, sys
want = sys.argv[1]
needs = json.load(sys.stdin)
print(", ".join(name for name, job in needs.items() if job.get("result") == want))' "$1"
}

run_is_superseded() {
  if [[ -z "${RUN_SHA:-}" ]]; then
    echo "check-pipeline-status: RUN_SHA is not set; a cancelled job cannot be proven superseded, so it is treated as a real failure" >&2
    return 1
  fi
  if [[ -z "${BRANCH_REF:-}" ]]; then
    echo "check-pipeline-status: BRANCH_REF is not set; a cancelled job cannot be proven superseded, so it is treated as a real failure" >&2
    return 1
  fi

  local head_sha=""
  if [[ -n "${BRANCH_HEAD_SHA:-}" ]]; then
    head_sha="$BRANCH_HEAD_SHA"
  elif ! head_sha="$(git ls-remote origin "refs/heads/${BRANCH_REF}" 2>/dev/null | awk '{print $1}')" || [[ -z "$head_sha" ]]; then
    echo "check-pipeline-status: could not resolve refs/heads/${BRANCH_REF} on origin; a cancelled job cannot be proven superseded, so it is treated as a real failure" >&2
    return 1
  fi

  [[ "$RUN_SHA" != "$head_sha" ]]
}

failed="$(select_by_result failure)"
cancelled="$(select_by_result cancelled)"

echo "Failed jobs:    ${failed:-<none>}"
echo "Cancelled jobs: ${cancelled:-<none>}"

status=0
if [[ -n "$failed" ]]; then
  echo "::error::Pipeline failed. Failing jobs: ${failed}"
  status=1
fi

if [[ -n "$cancelled" ]]; then
  if [[ "$IS_MAIN" != "true" ]]; then
    echo "::warning::Cancelled jobs: ${cancelled}. On a non-default ref this is usually a superseded run — open each job and search for 'has exceeded the maximum execution time' to rule out a real timeout. Steps wrapped in scripts/run-with-budget-warning.sh report a real timeout as a failure instead, which is why long steps carry their own budget."
  elif run_is_superseded; then
    echo "::warning::Cancelled jobs: ${cancelled}. This run is no longer the head of ${BRANCH_REF}: a newer commit superseded it, so the cancellation is expected churn."
  else
    echo "::error::Pipeline has cancelled jobs on main: ${cancelled}. This run was still the head of ${BRANCH_REF:-the branch} when the jobs were cancelled, so the cancellation is a real failure -- most likely a job killed by 'timeout-minutes', which GitHub reports as cancelled. Steps wrapped in scripts/run-with-budget-warning.sh report a real timeout as a failure instead, which is why long steps carry their own budget."
    status=1
  fi
fi

exit "$status"
