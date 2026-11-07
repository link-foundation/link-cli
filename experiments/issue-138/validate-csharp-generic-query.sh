#!/usr/bin/env bash
# Complete selected-address verification; never overlap another compiler job.
set -euo pipefail
cd "$(dirname "$0")/../.."
prefix=${VALIDATION_PREFIX:-experiments/issue-138/csharp-generic-query}
export DOTNET_ROOT="$PWD/experiments/issue-138/tools/dotnet-10.0.403"
export PATH="$DOTNET_ROOT:$PATH"
export DOTNET_GCHeapHardLimit=0x28000000 MSBUILDDISABLENODEREUSE=1
printf 'START %s\n' "$(date -u +%FT%TZ)"
dotnet format csharp --no-restore --verify-no-changes > "$prefix-format.log" 2>&1
bash experiments/issue-138/run-csharp-generic-query.sh > "$prefix-after.log" 2>&1
zero_log=${VALIDATION_PREFIX:-experiments/issue-138/csharp-query}-zero-after.log
bash experiments/issue-138/run-csharp-query-zero.sh > "$zero_log" 2>&1
for mode in Debug Release; do
  results=$(mktemp -d "$PWD/$prefix-$mode-results.XXXXXX")
  printf '%s\n' "$results" > "$prefix-$mode-results-path.txt"
  dotnet test csharp --configuration "$mode" --disable-build-servers -m:1 \
    --property:UseSharedCompilation=false \
    --collect:'XPlat Code Coverage;Format=json,cobertura' \
    --results-directory "$results" > "$prefix-$mode-tests.log" 2>&1
  mapfile -t reports < <(find "$results" -name coverage.json)
  test "${#reports[@]}" -eq 1
  python3 experiments/issue-138/check-causal-raw-coverage.py "${reports[0]}" \
    --require-session > "$prefix-$mode-causal-coverage.log" 2>&1
  printf '%s PASS %s\n' "$mode" "$(date -u +%FT%TZ)"
done
dotnet build-server shutdown > "$prefix-build-server-shutdown.log" 2>&1
VALIDATION_PREFIX="$prefix" bash experiments/issue-138/validate-csharp-query-gates.sh
printf 'ALL PASS %s\n' "$(date -u +%FT%TZ)"
