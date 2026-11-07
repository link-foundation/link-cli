#!/usr/bin/env bash
# Focused bounded checks; complete suites and actual-WASM execution stay in CI.
set -euo pipefail
cd "$(dirname "$0")/../.."
python3 experiments/issue-138/author-fence-model.py experiments/issue-138/author-fence-regenerated.json
cmp docs/protocol/author-fence-v1.json experiments/issue-138/author-fence-regenerated.json
node --max-old-space-size=128 --test --test-concurrency=1 --test-timeout=30000 \
  --experimental-test-coverage --test-coverage-include=js/client/src/engine-author-fence.js \
  --test-coverage-lines=100 --test-coverage-branches=100 --test-coverage-functions=100 \
  js/client/test/engine-author-fence.test.mjs
export CARGO_BUILD_JOBS=1 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 CARGO_INCREMENTAL=0
export RUSTFLAGS="${RUSTFLAGS:--C link-arg=-Wl,--threads=1}"
cargo test --manifest-path rust/Cargo.toml --features author-fencing,causal-batches,address-reservations \
  --test engine_author_fence_tests -- --test-threads=1
export DOTNET_PROCESSOR_COUNT=2 DOTNET_GCHeapHardLimit=0x40000000 MSBUILDDISABLENODEREUSE=1
dotnet test csharp/Foundation.Data.Doublets.Cli.Tests/Foundation.Data.Doublets.Cli.Tests.csproj \
  -c Release -m:1 -p:UseSharedCompilation=false --filter FullyQualifiedName~AuthorFenceTests
