#!/usr/bin/env bash
# Use the same public semantic tests natively and in bounded actual WASM.
set -euo pipefail
cd "$(dirname "$0")/../.."
export CARGO_BUILD_JOBS=1 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0
export PATH="$PWD/experiments/issue-138/tools/wasm-bindgen-0.2.129-x86_64-unknown-linux-musl:$PWD/experiments/issue-138/tools:$PATH"
cargo fmt --manifest-path rust/Cargo.toml --all -- --check
cargo fmt --manifest-path rust/wasm/Cargo.toml -- --check
cargo clippy --manifest-path rust/Cargo.toml --all-features --all-targets -- -D warnings > experiments/issue-138/arguments-final-clippy.log 2>&1
cargo test --manifest-path rust/Cargo.toml --all-features --test cli_argument_contract_tests > experiments/issue-138/arguments-final-focused.log 2>&1
cargo clippy --manifest-path rust/wasm/Cargo.toml --target wasm32-unknown-unknown --all-targets -- -D warnings > experiments/issue-138/arguments-wasm-clippy.log 2>&1
export WASM_COVERAGE_EXTRA_RUSTFLAGS=-Clink-arg=--max-memory=268435456
export CARGO_LLVM_COV_BIN="$PWD/experiments/issue-138/tools/cargo-llvm-cov"
bash experiments/issue-138/wasm-coverage.sh > experiments/issue-138/arguments-wasm-full.log 2>&1
cp experiments/issue-138/wasm-coverage.json experiments/issue-138/arguments-wasm-coverage.json
printf 'Complete actual WASM with coverage: passed\n'
