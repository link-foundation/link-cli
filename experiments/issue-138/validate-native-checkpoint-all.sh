#!/usr/bin/env bash
# Run each independent stage sequentially; retain every complete report.
set -euo pipefail
cd "$(dirname "$0")/../.."
for stage in csharp native wasm js cluster interop repository; do
 bash experiments/issue-138/validate-native-checkpoint.sh "$stage" > "experiments/issue-138/native-checkpoint-$stage-validation.log" 2>&1
 printf '%s\n' "Native-checkpoint $stage complete"
done
