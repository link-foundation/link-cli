#!/usr/bin/env bash
# Do not modify this driver while it is running. All compiler stages serialize.
set -euo pipefail
cd "$(dirname "$0")/../.."
for stage in native csharp wasm js cluster interop repository; do
 printf '%s: starting %s\n' "$(date -u +%FT%TZ)" "$stage"
 bash experiments/issue-138/validate-mirrored-root.sh "$stage" > "experiments/issue-138/mirrored-root-$stage-stage.log" 2>&1
 printf '%s: passed %s\n' "$(date -u +%FT%TZ)" "$stage"
done
