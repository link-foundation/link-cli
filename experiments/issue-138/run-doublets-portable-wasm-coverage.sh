#!/usr/bin/env bash
# Execute only shared heap-doublets contracts; retain all linked production
# sources in the resulting report. Full suites remain in CI.
set -euo pipefail
cd "$(dirname "$0")/../.."
export RUSTUP_TOOLCHAIN="${WASM_COVERAGE_TOOLCHAIN:-1.98.0}"
export RUST_LOG=warn RUSTC_BOOTSTRAP=1
export CARGO_BUILD_JOBS=1 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 CARGO_INCREMENTAL=0
export WASM_BINDGEN_TEST_TIMEOUT=30
export CARGO_TARGET_WASM32_UNKNOWN_UNKNOWN_RUNNER=wasm-bindgen-test-runner
export CARGO_TARGET_WASM32_UNKNOWN_UNKNOWN_RUSTFLAGS='-Cinstrument-coverage -Zcoverage-options=branch -Zno-profiler-runtime -Clink-args=--no-gc-sections --cfg=wasm_bindgen_unstable_test_coverage'
export NODE_OPTIONS=--max-old-space-size=256
prefix=experiments/issue-138/doublets-portable-wasm
cargo llvm-cov clean --manifest-path rust/wasm/Cargo.toml --workspace > "$prefix-clean.log" 2>&1
cargo llvm-cov test --manifest-path rust/wasm/Cargo.toml --target wasm32-unknown-unknown \
    --test storage --branch --json --output-path "$prefix.json" doublets_portable \
    > "$prefix-tests.log" 2>&1
