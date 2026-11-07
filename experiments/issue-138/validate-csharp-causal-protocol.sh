#!/usr/bin/env bash
# Keep local work focused; the unchanged CI matrix runs complete platform suites.
set -euo pipefail
cd "$(dirname "$0")/../.."
prefix=${VALIDATION_PREFIX:-experiments/issue-138/csharp-causal-protocol}
bash experiments/issue-138/run-csharp-causal-protocol.sh > "$prefix-focused.log" 2>&1
export DOTNET_PROCESSOR_COUNT=2 DOTNET_GCHeapHardLimit=0x40000000
dotnet format csharp --no-restore --verify-no-changes > "$prefix-format.log" 2>&1
VALIDATION_PREFIX="$prefix" bash experiments/issue-138/validate-csharp-query-gates.sh
