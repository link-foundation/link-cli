#!/usr/bin/env bash
# Shared fixture has sixteen cases, at most a few rows each; run alone.
set -euo pipefail
cd "$(dirname "$0")/../.."
export DOTNET_ROOT="$PWD/experiments/issue-138/tools/dotnet-10.0.403"
export PATH="$DOTNET_ROOT:$PATH"
export DOTNET_GCHeapHardLimit=0x28000000 MSBUILDDISABLENODEREUSE=1
dotnet run --project experiments/issue-138/csharp-causal-query/Probe.csproj \
  --configuration Release --property:UseSharedCompilation=false --property:BuildInParallel=false --no-launch-profile
