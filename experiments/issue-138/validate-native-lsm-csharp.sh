#!/usr/bin/env bash
# Sequential builds stay within the shared container memory budget.
set -euo pipefail
cd "$(dirname "$0")/../.."
export DOTNET_GCHeapHardLimit=0x28000000
prefix=experiments/issue-138/native-lsm-csharp
printf 'START %s\n' "$(date -u +%FT%TZ)"
dotnet format csharp --no-restore --verify-no-changes > "$prefix-format-check.log" 2>&1
for mode in Debug Release; do
 dotnet test csharp --configuration "$mode" --disable-build-servers -m:1 --collect:'XPlat Code Coverage' --results-directory "$PWD/$prefix-$mode-results" > "$prefix-$mode-full.log" 2>&1
 printf 'PASS %s %s\n' "$mode" "$(date -u +%FT%TZ)"
done
dotnet build examples/cluster-csharp --configuration Release --disable-build-servers -m:1 > "$prefix-example-build.log" 2>&1
for mode in direct partitioned reclaiming; do
 dotnet run --project examples/cluster-csharp --configuration Release --no-build -- --native-lsm "$mode" > "$prefix-consumer-$mode.log" 2>&1
done
python3 examples/native-lsm-js/run-interop.py > "$prefix-interop.log" 2>&1
node .github/scripts/check-sqlite-optional.mjs csharp > "$prefix-optional.log" 2>&1
printf 'ALL PASSED %s\n' "$(date -u +%FT%TZ)"
