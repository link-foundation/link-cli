#!/usr/bin/env bash
# Retained public C# processors: finite inputs, complete coverage denominators.
set -euo pipefail
cd "$(dirname "$0")/../.."
export DOTNET_GCHeapHardLimit=0x28000000
for mode in Debug Release; do
  dotnet test csharp --configuration "$mode" --disable-build-servers -m:1 --collect:'XPlat Code Coverage' --results-directory "$PWD/experiments/issue-138/query-csharp-$mode-results" > "experiments/issue-138/query-csharp-$mode-full.log" 2>&1
  printf 'Complete C# %s with coverage: passed\n' "$mode"
done
dotnet format csharp --no-restore --verify-no-changes > experiments/issue-138/query-csharp-format-check.log 2>&1
node csharp/scripts/validate-changeset.mjs > experiments/issue-138/query-changeset.log 2>&1
node csharp/scripts/check-file-size.mjs --lang all > experiments/issue-138/query-file-size.log 2>&1
node --test .github/scripts/*.test.mjs js/test/*.test.mjs csharp/scripts/*.test.mjs > experiments/issue-138/query-repository.log 2>&1
git diff --check
