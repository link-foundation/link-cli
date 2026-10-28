#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/../.."
export CARGO_BUILD_JOBS=1 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 DOTNET_GCHeapHardLimit=0x28000000
export PATH="$PWD/experiments/issue-138/tools/wasm-bindgen-0.2.129-x86_64-unknown-linux-musl:$PWD/experiments/issue-138/tools:$PATH"
expect_failure() {
 local label="$1"; shift
 if "$@" > "experiments/issue-138/engine-compaction-$label-before.log" 2>&1; then
  printf 'Unexpected passing before-feature control: %s\n' "$label" >&2; return 1
 fi
 printf 'Expected missing-feature failure: %s\n' "$label"
}
expect_failure native cargo test --manifest-path rust/Cargo.toml --no-default-features --test engine_compaction_tests
expect_failure csharp dotnet test csharp --configuration Debug --disable-build-servers -m:1 --filter FullyQualifiedName~EngineCompactBefore
expect_failure js node --test js/client/test/engine-compaction.test.mjs
expect_failure wasm cargo test --manifest-path rust/wasm/Cargo.toml --target wasm32-unknown-unknown --test engine_compaction
