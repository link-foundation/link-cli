#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/../.."
export CARGO_BUILD_JOBS=1 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0
export DOTNET_GCHeapHardLimit=0x28000000
export PATH="$PWD/experiments/issue-138/tools/dotnet-10.0.403:$PWD/experiments/issue-138/tools:$PWD/experiments/issue-138/tools/wasm-bindgen-0.2.129-x86_64-unknown-linux-musl:$PATH"
prefix=experiments/issue-138/causal-batch
printf 'START %s\n' "$(date -u +%FT%TZ)"
python3 experiments/issue-138/causal-batch-model.py > "$prefix-model.log" 2>&1
python3 experiments/issue-138/generate-causal-batch-vectors.py "$prefix-vectors-regenerated.json" > "$prefix-vectors.log" 2>&1
cmp docs/protocol/causal-batch-v1.json "$prefix-vectors-regenerated.json"
dotnet test csharp/Foundation.Data.Doublets.Cli.Tests --no-restore --filter FullyQualifiedName~CausalBatchTests \
  --collect:'XPlat Code Coverage' --results-directory "$prefix-csharp-focused-coverage" > "$prefix-csharp-focused.log" 2>&1
printf 'CSHARP PASS %s\n' "$(date -u +%FT%TZ)"
cargo fmt --manifest-path rust/Cargo.toml --all -- --check > "$prefix-format.log" 2>&1
cargo fmt --manifest-path rust/wasm/Cargo.toml --all -- --check >> "$prefix-format.log" 2>&1
cargo test --manifest-path rust/Cargo.toml --no-default-features --features causal-batches --test engine_causal_batch_tests > "$prefix-native-focused.log" 2>&1
CARGO_TARGET_WASM32_UNKNOWN_UNKNOWN_RUNNER=wasm-bindgen-test-runner WASM_BINDGEN_TEST_TIMEOUT=30 \
  cargo test --manifest-path rust/wasm/Cargo.toml --target wasm32-unknown-unknown --test core causal_batch > "$prefix-wasm-focused.log" 2>&1
printf 'RUST AND ACTUAL WASM PASS %s\n' "$(date -u +%FT%TZ)"
(cd js/client; npm run typecheck) > "$prefix-types.log" 2>&1
bun test js/client/test/engine-causal-batch.test.mjs > "$prefix-bun.log" 2>&1
deno test --allow-all --v8-flags=--max-old-space-size=128 js/client/test/engine-causal-batch.test.mjs > "$prefix-deno.log" 2>&1
printf 'ALL FOCUSED PASS %s\n' "$(date -u +%FT%TZ)"
