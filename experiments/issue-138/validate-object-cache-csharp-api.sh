#!/usr/bin/env bash
# Final ABI correction was made after the first full C# builds began.
# Run only after validate-object-cache.sh finishes; keep compilers sequential.
set -euo pipefail
cd "$(dirname "$0")/../.."
export DOTNET_GCHeapHardLimit=0x28000000
dotnet format csharp --no-restore --verify-no-changes > experiments/issue-138/object-cache-format-api.log 2>&1
for mode in Debug Release; do
 cp "experiments/issue-138/object-cache-csharp-$mode-full.log" "experiments/issue-138/object-cache-csharp-$mode-pre-api.log"
 dotnet test csharp --configuration "$mode" --disable-build-servers -m:1 --collect:'XPlat Code Coverage' --results-directory "$PWD/experiments/issue-138/object-cache-$mode-results" > "experiments/issue-138/object-cache-csharp-$mode-full.log" 2>&1
 printf 'Final C# %s with original constructor ABI and coverage: passed\n' "$mode"
done
