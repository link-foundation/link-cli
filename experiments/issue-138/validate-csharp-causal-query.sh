#!/usr/bin/env bash
# Run alone: finite shared cases, then complete Debug/Release coverage and gates.
set -euo pipefail
cd "$(dirname "$0")/../.."
prefix=experiments/issue-138/csharp-causal-query
bash experiments/issue-138/run-csharp-causal-query.sh > "$prefix-vectors-after.log" 2>&1
VALIDATION_PREFIX="$prefix" bash experiments/issue-138/validate-csharp-generic-query.sh
