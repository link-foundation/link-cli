#!/usr/bin/env bash
# Sequential single-compiler validation for the optional native page component.
set -euo pipefail
cd "$(dirname "$0")/../.."
export CARGO_BUILD_JOBS=1 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0
export PATH="$PWD/experiments/issue-138/tools:$PWD/experiments/issue-138/tools/wasm-bindgen-0.2.129-x86_64-unknown-linux-musl:$PATH"
export CARGO_TARGET_WASM32_UNKNOWN_UNKNOWN_RUNNER=wasm-bindgen-test-runner WASM_BINDGEN_TEST_TIMEOUT=30 RUST_LOG=warn
cargo fmt --manifest-path rust/Cargo.toml --all -- --check
cargo fmt --manifest-path rust/wasm/Cargo.toml --all -- --check
cargo clippy --manifest-path rust/Cargo.toml --all-features --all-targets -- -D warnings > experiments/issue-138/paged-rust-clippy.log 2>&1
cargo clippy --manifest-path rust/wasm/Cargo.toml --target wasm32-unknown-unknown --all-targets -- -D warnings > experiments/issue-138/paged-wasm-clippy.log 2>&1
RUSTC_BOOTSTRAP=1 cargo llvm-cov test --manifest-path rust/Cargo.toml --all-features --branch --json --output-path experiments/issue-138/paged-rust-coverage.json > experiments/issue-138/paged-rust-full.log 2>&1
WASM_COVERAGE_EXTRA_RUSTFLAGS=-Clink-arg=--max-memory=268435456 CARGO_LLVM_COV_BIN="$PWD/experiments/issue-138/tools/cargo-llvm-cov" bash experiments/issue-138/wasm-coverage.sh > experiments/issue-138/paged-wasm-full.log 2>&1
cp experiments/issue-138/wasm-coverage.json experiments/issue-138/paged-wasm-coverage.json
node .github/scripts/check-sqlite-optional.mjs rust > experiments/issue-138/paged-rust-optional.log 2>&1
cargo build --manifest-path rust/Cargo.toml --no-default-features --features native-pages --example engine_native_pages > experiments/issue-138/paged-rust-example-build.log 2>&1
: "${CLINK_WASM:?provide an existing actual-WASM web package}"
bash examples/native-pages/run-interop.sh > experiments/issue-138/paged-rust-js-interop.log 2>&1
