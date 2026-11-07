#!/usr/bin/env bash
# Reproduce original controls against immutable pre-feature source, sequentially.
set -euo pipefail
cd "$(dirname "$0")/../.."
export CARGO_BUILD_JOBS=1 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0
export DOTNET_GCHeapHardLimit=0x28000000
baseline=$(mktemp -d "$PWD/experiments/issue-138/chunks2-before.XXXXXX")
git archive 58d35f8800f92b81342d97ed982459960a4d86d7 | tar -x -C "$baseline"
cp experiments/issue-138/chunk-reclamation-api.test.mjs "$baseline/experiments/issue-138/"
cp experiments/issue-138/chunk-reclamation-api.rs "$baseline/rust/tests/engine_reclaiming/chunks.rs"
printf '\n#[path = "engine_reclaiming/chunks.rs"]\nmod chunks;\n' >> "$baseline/rust/tests/engine_reclaiming_tests.rs"
cp experiments/issue-138/ChunkReclamationApiTests.cs "$baseline/csharp/Foundation.Data.Doublets.Cli.Tests/"
expect_failure() {
 local label=$1 pattern=$2
 shift 2
 if "$@" > "$baseline/$label.log" 2>&1; then
  printf 'Unexpected successful baseline: %s\n' "$label" >&2; exit 1
 fi
 rg -q "$pattern" "$baseline/$label.log"
 printf 'PASS original %s failure; %s/%s.log\n' "$label" "$baseline" "$label"
}
expect_failure js 'chunks2' node --test "$baseline/experiments/issue-138/chunk-reclamation-api.test.mjs"
rg -q 'blob2' "$baseline/js.log"
expect_failure rust 'E0599.*open_chunked' cargo test --manifest-path "$baseline/rust/Cargo.toml" --target-dir "$PWD/rust/target" --test engine_reclaiming_tests selected_chunks_reuse_within_epoch_and_restage_after_fence
expect_failure csharp 'CS1501.*Open.*5 arguments' dotnet test "$baseline/csharp" --disable-build-servers -m:1 --filter FullyQualifiedName~SelectedChunksReuseWithinEpochAndRestageAfterFence
printf 'All three chunks2 baseline failures reproduced from clean 58d35f8 source.\n'
