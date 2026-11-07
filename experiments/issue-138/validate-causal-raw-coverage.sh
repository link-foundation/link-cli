#!/usr/bin/env bash
# Preserve complete C# denominators; exercise the shared vector in every port.
set -euo pipefail
cd "$(dirname "$0")/../.."
export CARGO_BUILD_JOBS=1 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0
export DOTNET_GCHeapHardLimit=0x28000000
export PATH="$PWD/experiments/issue-138/tools/dotnet-10.0.403:$PWD/experiments/issue-138/tools:$PWD/experiments/issue-138/tools/wasm-bindgen-0.2.129-x86_64-unknown-linux-musl:$PATH"
prefix=experiments/issue-138/causal-raw
printf 'START %s\n' "$(date -u +%FT%TZ)"
python3 experiments/issue-138/generate-causal-batch-vectors.py "$prefix-vectors-regenerated.json" > "$prefix-vectors.log" 2>&1
cmp docs/protocol/causal-batch-v1.json "$prefix-vectors-regenerated.json"
for mode in Debug Release; do
  dotnet test csharp --configuration "$mode" --disable-build-servers -m:1 \
    --collect:'XPlat Code Coverage;Format=json,cobertura' \
    --results-directory "$PWD/$prefix-$mode-results" > "$prefix-csharp-$mode.log" 2>&1
  report=$(find "$prefix-$mode-results" -name coverage.json -type f)
  python3 experiments/issue-138/check-causal-raw-coverage.py "$report" > "$prefix-$mode-records.log" 2>&1
  printf 'CSHARP %s PASS %s\n' "$mode" "$(date -u +%FT%TZ)"
done
dotnet build-server shutdown --vbcscompiler > "$prefix-build-server.log" 2>&1
cargo test --manifest-path rust/Cargo.toml --no-default-features --features causal-batches \
  --test engine_causal_batch_tests > "$prefix-native.log" 2>&1
CARGO_TARGET_WASM32_UNKNOWN_UNKNOWN_RUNNER=wasm-bindgen-test-runner WASM_BINDGEN_TEST_TIMEOUT=30 \
  cargo test --manifest-path rust/wasm/Cargo.toml --target wasm32-unknown-unknown --test core causal_batch > "$prefix-wasm.log" 2>&1
printf 'NATIVE AND ACTUAL WASM PASS %s\n' "$(date -u +%FT%TZ)"
(cd js/client; node --max-old-space-size=256 --test --test-concurrency=1 --test-timeout=30000 \
  --experimental-test-coverage --test-coverage-include='src/**' \
  --test-coverage-lines=100 --test-coverage-branches=100 --test-coverage-functions=100 \
  test/*.test.mjs) > "$prefix-node.log" 2>&1
bun test js/client/test/engine-causal-batch.test.mjs > "$prefix-bun.log" 2>&1
deno test --allow-all --v8-flags=--max-old-space-size=128 js/client/test/engine-causal-batch.test.mjs > "$prefix-deno.log" 2>&1
npm run test:js --prefix js > "$prefix-repository.log" 2>&1
PATH="$PWD/experiments/issue-138/tools/shellcheck-v0.11.0:$PATH" experiments/issue-138/tools/actionlint > "$prefix-workflows.log" 2>&1
experiments/issue-138/tools/shellcheck-v0.11.0/shellcheck experiments/issue-138/validate-causal-raw-coverage.sh >> "$prefix-workflows.log" 2>&1
git diff --check > "$prefix-whitespace.log" 2>&1
printf 'ALL PASS %s\n' "$(date -u +%FT%TZ)"
