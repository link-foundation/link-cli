#!/usr/bin/env bash
# The additional non-replaceable inner-log assertion was found in coverage review.
# Run after the complete sequential validator to avoid overlapping compilers.
set -euo pipefail
cd "$(dirname "$0")/../.."
export DOTNET_GCHeapHardLimit=0x28000000
export PATH="$PWD/experiments/issue-138/tools/dotnet-10.0.403:$PATH"
dotnet format csharp --no-restore --verify-no-changes > experiments/issue-138/engine-compaction-csharp-followup-format.log 2>&1
for mode in Debug Release; do
 dotnet test csharp --configuration "$mode" --disable-build-servers -m:1 --filter 'FullyQualifiedName~Delta|FullyQualifiedName~EngineCompact' --collect:'XPlat Code Coverage' --results-directory "$PWD/experiments/issue-138/engine-compaction-csharp-$mode-followup-results" > "experiments/issue-138/engine-compaction-csharp-$mode-followup.log" 2>&1
 printf 'C# %s final delta/compaction coverage controls: passed\n' "$mode"
done
