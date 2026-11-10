#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/../.."
export PATH="$PWD/experiments/issue-138/tools:$PWD/experiments/issue-138/tools/wasm-bindgen-0.2.129-x86_64-unknown-linux-musl:$PATH"
prefix=experiments/issue-138/causal-session
printf 'START %s\n' "$(date -u +%FT%TZ)"
python3 experiments/issue-138/build-native-lsm-wasm.py > "$prefix-binding-build.log" 2>&1
printf 'BINDING BUILD PASS %s\n' "$(date -u +%FT%TZ)"
node --max-old-space-size=256 experiments/issue-138/run-causal-session-actual.mjs > "$prefix-actual-node.log" 2>&1
bun experiments/issue-138/run-causal-session-actual.mjs > "$prefix-actual-bun.log" 2>&1
deno run --allow-read --allow-env --v8-flags=--max-old-space-size=128 experiments/issue-138/run-causal-session-actual.mjs > "$prefix-actual-deno.log" 2>&1
printf 'ALL ACTUAL PASS %s\n' "$(date -u +%FT%TZ)"
