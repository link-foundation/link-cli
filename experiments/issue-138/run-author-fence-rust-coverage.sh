#!/usr/bin/env bash
# Focus one contract target; complete native/WASM suites remain in CI.
# Requires cargo-llvm-cov 0.9.1 and the toolchain's llvm-tools-preview component.
set -euo pipefail
cd "$(dirname "$0")/../.."
export CARGO_BUILD_JOBS=1 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 CARGO_INCREMENTAL=0
export RUSTC_BOOTSTRAP=1
export RUSTFLAGS="${RUSTFLAGS:--C link-arg=-Wl,--threads=1}"
prefix=experiments/issue-138/author-fence-native-coverage
# Remove previous workspace coverage objects/profiles so different feature
# builds are not mixed. Dependency compilation remains cached.
cargo llvm-cov clean --manifest-path rust/Cargo.toml --workspace > "$prefix-clean.log" 2>&1
cargo llvm-cov test --manifest-path rust/Cargo.toml \
  --features author-fencing,causal-batches,address-reservations \
  --test engine_author_fence_tests --branch --no-report -- --test-threads=1 > "$prefix-tests.log" 2>&1
cargo llvm-cov report --manifest-path rust/Cargo.toml --branch --json --summary-only \
  --output-path "$prefix.json" > "$prefix-report.log" 2>&1
python3 experiments/issue-138/check-author-fence-rust-coverage.py "$prefix.json"
