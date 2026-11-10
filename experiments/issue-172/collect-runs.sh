#!/usr/bin/env bash
# Collect metadata, jobs, annotations and complete logs for workflow runs.
# Usage: collect-runs.sh <evidence-dir> <run-id>...
set -euo pipefail
out=$1; shift
repo=link-foundation/link-cli
mkdir -p "$out/github" "$out/ci-logs"
for run in "$@"; do
  gh api "repos/$repo/actions/runs/$run" > "$out/github/run-$run.json"
  gh api "repos/$repo/actions/runs/$run/jobs?per_page=100&filter=all" --paginate > "$out/github/run-$run-jobs.json"
  # Annotations carry warnings that never fail a job (hidden false negatives).
  for job in $(jq -r '.jobs[].id' "$out/github/run-$run-jobs.json"); do
    gh api "repos/$repo/check-runs/$job/annotations?per_page=100" --paginate > "$out/github/annotations-$job.json" 2>/dev/null || rm -f "$out/github/annotations-$job.json"
  done
  if ! [ -s "$out/ci-logs/run-$run.log.gz" ]; then
    if gh run view "$run" --repo "$repo" --log > "$out/ci-logs/run-$run.log" 2> "$out/ci-logs/run-$run.err"; then
      gzip -9f "$out/ci-logs/run-$run.log"; rm -f "$out/ci-logs/run-$run.err"
    else
      # Logs of an individual failed/cancelled job are still available by job id.
      rm -f "$out/ci-logs/run-$run.log"
      for job in $(jq -r '.jobs[] | select(.conclusion != "skipped") | .id' "$out/github/run-$run-jobs.json"); do
        gh api "repos/$repo/actions/jobs/$job/logs" > "$out/ci-logs/run-$run-job-$job.log" 2>>"$out/ci-logs/run-$run.err" \
          && gzip -9f "$out/ci-logs/run-$run-job-$job.log" || rm -f "$out/ci-logs/run-$run-job-$job.log"
      done
    fi
  fi
  echo "$run $(jq -r '"\(.name) \(.status) \(.conclusion) \(.head_sha[0:7]) \(.created_at)"' "$out/github/run-$run.json")"
done
