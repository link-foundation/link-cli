#!/usr/bin/env bash
# Supplement full-suite profiles with the missing finite admission control.
# Run only after validate-causal-batch.sh has finished and retained its profiles.
set -euo pipefail
cd "$(dirname "$0")/../.."
export CARGO_BUILD_JOBS=1 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0
export DOTNET_GCHeapHardLimit=0x28000000
export PATH="$PWD/experiments/issue-138/tools/dotnet-10.0.403:$PWD/experiments/issue-138/tools:$PWD/experiments/issue-138/tools/wasm-bindgen-0.2.129-x86_64-unknown-linux-musl:$PATH"
prefix=experiments/issue-138/causal-batch
printf 'ADMISSION START %s\n' "$(date -u +%FT%TZ)"
bash -c "$(cat experiments/issue-138/validate-causal-batch-csharp.sh)" \
  experiments/issue-138/validate-causal-batch-csharp.sh "${1:-all}"
cargo fmt --manifest-path rust/Cargo.toml --check > "$prefix-admission-format.log" 2>&1
cargo fmt --manifest-path rust/wasm/Cargo.toml --check >> "$prefix-admission-format.log" 2>&1
RUSTC_BOOTSTRAP=1 cargo llvm-cov test --manifest-path rust/Cargo.toml --all-features \
  --test engine_causal_batch_tests --no-clean --branch --json \
  --output-path "$prefix-native-coverage.json" > "$prefix-native-admission.log" 2>&1
printf 'NATIVE ADMISSION PASS %s\n' "$(date -u +%FT%TZ)"
WASM_COVERAGE_EXTRA_RUSTFLAGS=-Clink-arg=--max-memory=268435456 \
  CARGO_LLVM_COV_BIN="$PWD/experiments/issue-138/tools/cargo-llvm-cov" \
  bash experiments/issue-138/wasm-coverage.sh > "$prefix-wasm-full.log" 2>&1
cp experiments/issue-138/wasm-coverage.json "$prefix-wasm-coverage.json"
printf 'ADMISSION PASS %s\n' "$(date -u +%FT%TZ)"
