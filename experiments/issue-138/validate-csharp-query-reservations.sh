#!/usr/bin/env bash
# Run alone: the full C# matrix uses one compiler and a 640 MiB managed heap.
set -euo pipefail
cd "$(dirname "$0")/../.."
prefix=experiments/issue-138/csharp-query-reservations
bash experiments/issue-138/run-csharp-query-allocation.sh > "$prefix-allocation-after.log" 2>&1
VALIDATION_PREFIX="$prefix" bash experiments/issue-138/validate-csharp-generic-query.sh
