#!/usr/bin/env bash
# Focus one feature/runtime at a time; the complete interchange matrix stays in CI.
set -euo pipefail
cd "$(dirname "$0")/../.."
for runtime in node rust csharp; do
  NATIVE_CAUSAL_LAYOUT=fence NATIVE_LSM_RUNTIMES="$runtime" \
    python3 examples/native-lsm-js/run-causal-faults.py
done
