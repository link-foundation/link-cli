#!/usr/bin/env bash
# Run after the JS stage, preserving one complete report per independent stage.
set -euo pipefail
cd "$(dirname "$0")/../.."
for stage in csharp native wasm cluster interop repository; do
 bash experiments/issue-138/validate-engine-compaction.sh "$stage" > "experiments/issue-138/engine-compaction-$stage-validation.log" 2>&1
 printf '%s\n' "Engine-compaction $stage complete"
done
