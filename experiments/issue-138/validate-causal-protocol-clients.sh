#!/usr/bin/env bash
# Actual current bindings and complete JS production coverage, sequentially.
set -euo pipefail
cd "$(dirname "$0")/../.."
prefix=experiments/issue-138/causal-protocol
export PATH="$PWD/experiments/issue-138/tools/wasm-bindgen-0.2.129-x86_64-unknown-linux-musl:$PATH"
(
  cd js/client
  node --max-old-space-size=256 --test --test-concurrency=1 --test-timeout=30000 \
    --experimental-test-coverage --test-coverage-include='src/**' \
    --test-coverage-lines=100 --test-coverage-branches=100 --test-coverage-functions=100 \
    test/*.test.mjs
) > "$prefix-js-final.log" 2>&1
npm run typecheck --prefix js/client > "$prefix-types.log" 2>&1
RUST_LOG=warn python3 experiments/issue-138/build-native-lsm-wasm.py > "$prefix-binding-build.log" 2>&1
node --max-old-space-size=256 experiments/issue-138/run-causal-session-actual.mjs > "$prefix-actual-node.log" 2>&1
bun experiments/issue-138/run-causal-session-actual.mjs > "$prefix-actual-bun.log" 2>&1
deno run --allow-read --allow-env --v8-flags=--max-old-space-size=128 \
  experiments/issue-138/run-causal-session-actual.mjs > "$prefix-actual-deno.log" 2>&1
experiments/issue-138/tools/shellcheck-v0.11.0/shellcheck \
  experiments/issue-138/validate-causal-protocol.sh \
  experiments/issue-138/validate-causal-protocol-clients.sh >> "$prefix-release-shellcheck.log" 2>&1
printf 'CLIENTS PASS %s\n' "$(date -u +%FT%TZ)"
