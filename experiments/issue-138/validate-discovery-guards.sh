#!/usr/bin/env bash
# Keep native compilers and complete coverage runs sequential under the cap.
set -euo pipefail
cd "$(dirname "$0")/../.."
export DOTNET_ROOT="$PWD/experiments/issue-138/tools/dotnet-10.0.403"
export PATH="$DOTNET_ROOT:$PWD/experiments/issue-138/tools:$PATH"
export DOTNET_GCHeapHardLimit=0x28000000
prefix=experiments/issue-138/discovery-guards
printf 'START %s\n' "$(date -u +%FT%TZ)"
dotnet format csharp --no-restore --verify-no-changes > "$prefix-format.log" 2>&1
dotnet test csharp/Foundation.Data.Doublets.Cli.Tests --configuration Debug --disable-build-servers -m:1 --filter FullyQualifiedName~NativeLsmReadOnlyContractsTests > "$prefix-focused.log" 2>&1
printf 'FOCUSED PASS %s\n' "$(date -u +%FT%TZ)"
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
experiments/issue-138/tools/shellcheck-v0.11.0/shellcheck experiments/issue-138/validate-discovery-guards.sh >> "$prefix-workflows.log" 2>&1
git diff --check > "$prefix-whitespace.log" 2>&1
printf 'ALL PASS %s\n' "$(date -u +%FT%TZ)"
