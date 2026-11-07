#!/usr/bin/env bash
# Finite real-quota rejection followed by full native configurations and checks.
set -euo pipefail
cd "$(dirname "$0")/../.."
export DOTNET_GCHeapHardLimit=0x28000000
dotnet test csharp --configuration Debug --disable-build-servers -m:1 --filter FullyQualifiedName~MappedGrowthTests > experiments/issue-138/mapped-growth-final-focused.log 2>&1
for mode in Debug Release; do
  dotnet test csharp --configuration "$mode" --disable-build-servers -m:1 --collect:'XPlat Code Coverage' --results-directory "$PWD/experiments/issue-138/mapped-growth-csharp-$mode-results" > "experiments/issue-138/mapped-growth-csharp-$mode-full.log" 2>&1
  printf 'Complete C# %s with coverage: passed\n' "$mode"
done
dotnet format csharp --no-restore --verify-no-changes > experiments/issue-138/mapped-growth-format-check.log 2>&1
node csharp/scripts/validate-changeset.mjs > experiments/issue-138/mapped-growth-changeset.log 2>&1
node csharp/scripts/check-file-size.mjs --lang all > experiments/issue-138/mapped-growth-file-size.log 2>&1
node --test .github/scripts/*.test.mjs js/test/*.test.mjs csharp/scripts/*.test.mjs > experiments/issue-138/mapped-growth-repository.log 2>&1
git diff --check
