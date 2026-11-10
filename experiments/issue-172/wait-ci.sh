#!/usr/bin/env bash
# Wait until every workflow run for a commit has completed, then print the conclusions.
# Usage: wait-ci.sh <sha> [branch]
sha=$1; branch=${2:-issue-172-4d10c904b01e}; repo=link-foundation/link-cli
while :; do
  runs=$(gh run list --repo "$repo" --branch "$branch" --limit 60 --json databaseId,name,status,conclusion,headSha \
    --jq "[.[] | select(.headSha == \"$sha\")]")
  total=$(jq length <<<"$runs"); pending=$(jq '[.[] | select(.status != "completed")] | length' <<<"$runs")
  if [ "$total" -gt 0 ] && [ "$pending" -eq 0 ]; then
    jq -r '.[] | "\(.databaseId) \(.name) \(.conclusion)"' <<<"$runs"; exit 0
  fi
  sleep 60
done
