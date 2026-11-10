#!/usr/bin/env bash
# Build serially, then reap every finite five-runtime physical-store worker.
set -euo pipefail
cd "$(dirname "$0")/../.."
export CARGO_BUILD_JOBS=1 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0
export DOTNET_GCHeapHardLimit=0x28000000
export PATH="$PWD/experiments/issue-138/tools/dotnet-10.0.403:$PWD/experiments/issue-138/tools:$PATH"
export CLINK_WASM=experiments/issue-138/native-lsm-wasm-package
prefix=experiments/issue-138/causal-session
printf 'START %s\n' "$(date -u +%FT%TZ)"
cargo fmt --manifest-path rust/Cargo.toml --check > "$prefix-physical-format.log" 2>&1
cargo clippy --manifest-path rust/Cargo.toml --all-features --all-targets -- -D warnings > "$prefix-physical-clippy.log" 2>&1
cargo build --manifest-path rust/Cargo.toml --no-default-features \
  --features native-pages,physical-lsm,causal-merkle,causal-batches,causal-sessions \
  --example native_lsm > "$prefix-physical-rust-build.log" 2>&1
dotnet build examples/cluster-csharp --configuration Release --disable-build-servers -m:1 > "$prefix-physical-csharp-build.log" 2>&1
NATIVE_LSM_LAYOUT=session python3 examples/native-lsm-js/run-interop.py > "$prefix-physical-handoffs.log" 2>&1
NATIVE_CAUSAL_LAYOUT=session python3 examples/native-lsm-js/run-causal-faults.py > "$prefix-process-faults.log" 2>&1
printf 'ALL PHYSICAL PASS %s\n' "$(date -u +%FT%TZ)"
