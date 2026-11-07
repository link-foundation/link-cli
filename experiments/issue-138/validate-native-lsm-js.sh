#!/usr/bin/env bash
# Run sequentially; all selected process and fault probes have finite inputs.
set -euo pipefail
cd "$(dirname "$0")/../.."
export CARGO_BUILD_JOBS=1 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0
prefix=experiments/issue-138/native-lsm
(cd js/client && node --test --test-concurrency=1 --test-timeout=30000 --experimental-test-coverage --test-coverage-include='src/**' --test-coverage-lines=100 --test-coverage-branches=100 --test-coverage-functions=100 test/*.test.mjs && npm run typecheck) > "$prefix-js-full.log" 2>&1
bun test js/client/test/engine-file-lsm-*.test.mjs --timeout 30000 > "$prefix-production-bun-full.log" 2>&1
deno test --allow-all js/client/test/engine-file-lsm-*.test.mjs > "$prefix-production-deno-full.log" 2>&1
node .github/scripts/check-sqlite-optional.mjs js > "$prefix-js-optional.log" 2>&1
node experiments/issue-138/verify-lsm-store-lazy.mjs > "$prefix-js-lazy.log" 2>&1
python3 experiments/issue-138/build-native-lsm-wasm.py > "$prefix-wasm-package.log" 2>&1
export CLINK_WASM=experiments/issue-138/native-lsm-wasm-package
python3 experiments/issue-138/verify-wasm-memory-cap.py "$CLINK_WASM/clink_wasm_bg.wasm" > "$prefix-wasm-cap.log"
node --max-old-space-size=256 examples/native-lsm-js/cluster.mjs > "$prefix-real-consumer-node.log" 2>&1
bun examples/native-lsm-js/cluster.mjs > "$prefix-real-consumer-bun.log" 2>&1
deno run --allow-all examples/native-lsm-js/cluster.mjs > "$prefix-real-consumer-deno.log" 2>&1
node csharp/scripts/check-file-size.mjs --lang all > "$prefix-file-size.log"
node csharp/scripts/validate-changeset.mjs > "$prefix-changeset.log"
node --test .github/scripts/*.test.mjs js/test/*.test.mjs csharp/scripts/*.test.mjs > "$prefix-repository.log" 2>&1
git diff --check
git diff --cached --check
printf 'ALL PASSED %s\n' "$(date -u +%FT%TZ)"
