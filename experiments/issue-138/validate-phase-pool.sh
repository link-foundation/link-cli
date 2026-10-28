#!/usr/bin/env bash
# Keep compilers and bounded process experiments sequential under the memory cap.
set -euo pipefail
cd "$(dirname "$0")/../.."
export DOTNET_ROOT="$PWD/experiments/issue-138/tools/dotnet-10.0.403"
export PATH="$DOTNET_ROOT:$PWD/experiments/issue-138/tools:$PATH"
export DOTNET_GCHeapHardLimit=0x28000000
prefix=experiments/issue-138/phase-pool
printf 'START %s\n' "$(date -u +%FT%TZ)"
bash experiments/issue-138/run-phase-pool-before.sh > "$prefix-baseline-reproduction.log" 2>&1
dotnet build experiments/issue-138/phase-pool-probe/PhasePoolProbe.csproj --configuration Release --disable-build-servers -m:1 > "$prefix-probe-build-after.log" 2>&1
dotnet experiments/issue-138/phase-pool-probe/bin/Release/net10.0/PhasePoolProbe.dll > "$prefix-probe-after.log" 2>&1
dotnet format csharp --no-restore --verify-no-changes > "$prefix-format.log" 2>&1
for mode in Debug Release; do
 dotnet test csharp --configuration "$mode" --disable-build-servers -m:1 --collect:'XPlat Code Coverage' --results-directory "$PWD/$prefix-$mode-results" > "$prefix-csharp-$mode-full.log" 2>&1
 printf 'CSHARP %s PASS %s\n' "$mode" "$(date -u +%FT%TZ)"
done
npm run test:js --prefix js > "$prefix-repository.log" 2>&1
npm run typecheck --prefix js/client > "$prefix-types.log" 2>&1
node csharp/scripts/check-file-size.mjs > "$prefix-source-sizes.log" 2>&1
node csharp/scripts/validate-changeset.mjs > "$prefix-changeset.log" 2>&1
GITHUB_TOKEN="$(gh auth token)" node .github/scripts/check-latest-dependencies.mjs > "$prefix-dependency-freshness.log" 2>&1
PATH="$PWD/experiments/issue-138/tools/shellcheck-v0.11.0:$PATH" experiments/issue-138/tools/actionlint > "$prefix-workflows.log" 2>&1
experiments/issue-138/tools/shellcheck-v0.11.0/shellcheck experiments/issue-138/{run-phase-pool-before,validate-phase-pool}.sh >> "$prefix-workflows.log" 2>&1
git diff --check > "$prefix-whitespace.log" 2>&1
printf 'ALL PASS %s\n' "$(date -u +%FT%TZ)"
