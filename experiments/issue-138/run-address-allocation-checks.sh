#!/usr/bin/env bash
# Bounded focused checks. Full runtime, native coverage and WASM suites run in CI.
set -euo pipefail
cd "$(dirname "$0")/../.."
mkdir -p experiments/issue-138/ci-logs
python3 experiments/issue-138/address-allocation-model.py experiments/issue-138/address-reservations-regenerated.json
cmp docs/protocol/address-reservations-v1.json experiments/issue-138/address-reservations-regenerated.json
node --max-old-space-size=128 --test --test-concurrency=1 --test-timeout=30000 \
  --experimental-test-coverage --test-coverage-include='js/client/src/engine-address-allocator.js' \
  --test-coverage-lines=100 --test-coverage-branches=100 --test-coverage-functions=100 \
  js/client/test/engine-address-allocator.test.mjs
export CARGO_BUILD_JOBS=1 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 CARGO_INCREMENTAL=0
export RUSTFLAGS="${RUSTFLAGS:--C link-arg=-Wl,--threads=1}"
cargo test --manifest-path rust/Cargo.toml --features address-reservations \
  --test engine_address_allocator_tests -- --test-threads=1
export DOTNET_PROCESSOR_COUNT=2 DOTNET_GCHeapHardLimit=0x40000000 MSBUILDDISABLENODEREUSE=1 DOTNET_CLI_TELEMETRY_OPTOUT=1
dotnet test csharp/Foundation.Data.Doublets.Cli.Tests/Foundation.Data.Doublets.Cli.Tests.csproj \
  --configuration Release -m:1 -p:UseSharedCompilation=false --filter FullyQualifiedName~AddressAllocatorTests
