#!/usr/bin/env bash
# Repeat complete C# evidence after the Release null-token coverage control.
set -euo pipefail
cd "$(dirname "$0")/../.."
export DOTNET_GCHeapHardLimit=0x28000000
export PATH="$PWD/experiments/issue-138/tools/dotnet-10.0.403:$PATH"
prefix=experiments/issue-138/causal-batch
printf 'CSHARP NULL-TOKEN START %s\n' "$(date -u +%FT%TZ)"
dotnet --version > "$prefix-dotnet-sdk.log"
case "${1:-all}" in
all)
modes=(Debug Release)
dotnet format csharp --no-restore --verify-no-changes > "$prefix-csharp-format.log" 2>&1
dotnet format examples/cluster-csharp/ClusterExample.csproj --no-restore --verify-no-changes >> "$prefix-csharp-format.log" 2>&1
dotnet test csharp/Foundation.Data.Doublets.Cli.Tests --no-restore --filter FullyQualifiedName~CausalBatchTests \
  --collect:'XPlat Code Coverage' --results-directory "$prefix-csharp-focused-coverage" > "$prefix-csharp-focused.log" 2>&1
dotnet build-server shutdown --vbcscompiler > "$prefix-build-server-shutdown.log" 2>&1
;;
Release) modes=(Release) ;; # Retain the completed Debug report and output.
*) printf 'usage: %s [all|Release]\n' "$0" >&2; exit 2 ;;
esac
for mode in "${modes[@]}"; do
  dotnet test csharp --configuration "$mode" --disable-build-servers -m:1 \
    --collect:'XPlat Code Coverage' --results-directory "$PWD/$prefix-$mode-results" > "$prefix-csharp-$mode-full.log" 2>&1
done
printf 'CSHARP NULL-TOKEN PASS %s\n' "$(date -u +%FT%TZ)"
