#!/usr/bin/env bash
# Run actual wasm-bindgen tests with the experimental WASM coverage runtime.
# Requires cargo-llvm-cov, matching wasm-bindgen-test-runner and LLVM tools.
# https://wasm-bindgen.github.io/wasm-bindgen/wasm-bindgen-test/coverage.html
set -euo pipefail
repo_root=$(cd "$(dirname "$0")/../.." && pwd)
cd "$repo_root"
# wasm-bindgen-test 0.3.79 pins minicov 0.3.8 (LLVM 22/profraw v10).
# LLVM 23 changes the runtime layout; pin only this coverage toolchain.
export RUSTUP_TOOLCHAIN="${WASM_COVERAGE_TOOLCHAIN:-1.98.0}"
export RUST_LOG="${WASM_COVERAGE_LOG:-warn}"
export CARGO_BUILD_JOBS=1
export CARGO_PROFILE_DEV_DEBUG=0
export CARGO_PROFILE_TEST_DEBUG=0
export RUSTC_BOOTSTRAP=1
export WASM_BINDGEN_TEST_TIMEOUT=30
export CARGO_TARGET_WASM32_UNKNOWN_UNKNOWN_RUNNER=wasm-bindgen-test-runner
export CARGO_TARGET_WASM32_UNKNOWN_UNKNOWN_RUSTFLAGS="-Cinstrument-coverage -Zcoverage-options=branch -Zno-profiler-runtime -Clink-args=--no-gc-sections --cfg=wasm_bindgen_unstable_test_coverage ${WASM_COVERAGE_EXTRA_RUSTFLAGS:-}"
"${CARGO_LLVM_COV_BIN:-cargo-llvm-cov}" llvm-cov test \
  --manifest-path rust/wasm/Cargo.toml --target wasm32-unknown-unknown \
  --test web --test engine --test group_commit --test core --test protocol --test storage --branch --json \
  --output-path "$repo_root/experiments/issue-138/wasm-coverage.json"
