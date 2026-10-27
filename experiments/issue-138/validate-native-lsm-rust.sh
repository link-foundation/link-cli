#!/usr/bin/env bash
# Keep native/.NET/WASM compilation sequential within the shared memory budget.
set -euo pipefail
cd "$(dirname "$0")/../.."
export CARGO_BUILD_JOBS=1 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0
export PATH="$PWD/experiments/issue-138/tools:$PWD/experiments/issue-138/tools/wasm-bindgen-0.2.129-x86_64-unknown-linux-musl:$PATH"
export DOTNET_ROOT="$PWD/experiments/issue-138/tools/dotnet-10.0.403"
export PATH="$DOTNET_ROOT:$PATH"
export DOTNET_GCHeapHardLimit=0x28000000
prefix=experiments/issue-138/native-lsm-rust
printf 'START %s\n' "$(date -u +%FT%TZ)"
cargo fmt --manifest-path rust/Cargo.toml --all -- --check > "$prefix-format.log" 2>&1
cargo fmt --manifest-path rust/wasm/Cargo.toml --all -- --check >> "$prefix-format.log" 2>&1
cargo clippy --manifest-path rust/Cargo.toml --all-features --all-targets -- -D warnings > "$prefix-clippy.log" 2>&1
cargo clippy --manifest-path rust/wasm/Cargo.toml --target wasm32-unknown-unknown --all-targets -- -D warnings > "$prefix-wasm-clippy.log" 2>&1
RUSTC_BOOTSTRAP=1 cargo llvm-cov test --manifest-path rust/Cargo.toml --all-features --branch --json --output-path "$prefix-coverage.json" > "$prefix-full.log" 2>&1
cargo test --manifest-path rust/Cargo.toml --all-features --doc > "$prefix-doctests.log" 2>&1
printf 'NATIVE PASS %s\n' "$(date -u +%FT%TZ)"
WASM_COVERAGE_EXTRA_RUSTFLAGS=-Clink-arg=--max-memory=268435456 CARGO_LLVM_COV_BIN="$PWD/experiments/issue-138/tools/cargo-llvm-cov" bash experiments/issue-138/wasm-coverage.sh > "$prefix-wasm-full.log" 2>&1
cp experiments/issue-138/wasm-coverage.json "$prefix-wasm-coverage.json"
node .github/scripts/check-sqlite-optional.mjs rust > "$prefix-optional.log" 2>&1
cargo build --manifest-path rust/Cargo.toml --no-default-features --features native-pages,physical-lsm --example native_lsm > "$prefix-example-build.log" 2>&1
for mode in direct partitioned reclaiming; do
 rust/target/debug/examples/native_lsm cluster "$mode" > "$prefix-consumer-$mode.log" 2>&1
done
python3 examples/native-lsm-js/run-interop.py > "$prefix-five-runtime-interop.log" 2>&1
printf 'ALL PASS %s\n' "$(date -u +%FT%TZ)"
