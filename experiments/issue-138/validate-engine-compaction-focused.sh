#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/../.."
export CARGO_BUILD_JOBS=1 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0
export DOTNET_GCHeapHardLimit=0x28000000
export RUST_LOG=warn
export PATH="$PWD/experiments/issue-138/tools/dotnet-10.0.403:$PWD/experiments/issue-138/tools/wasm-bindgen-0.2.129-x86_64-unknown-linux-musl:$PWD/experiments/issue-138/tools:$PATH"
cargo test --manifest-path rust/Cargo.toml --no-default-features --test engine_compaction_tests --test engine_compaction_concurrent_tests --test engine_checkpoint_pages_tests > experiments/issue-138/engine-compaction-native-focused.log 2>&1
printf 'Native compaction controls passed\n'
export CARGO_TARGET_WASM32_UNKNOWN_UNKNOWN_RUNNER=wasm-bindgen-test-runner
CARGO_TARGET_WASM32_UNKNOWN_UNKNOWN_RUSTFLAGS=-Clink-arg=--max-memory=268435456 cargo test --manifest-path rust/wasm/Cargo.toml --target wasm32-unknown-unknown --test core compaction > experiments/issue-138/engine-compaction-wasm-focused.log 2>&1
CARGO_TARGET_WASM32_UNKNOWN_UNKNOWN_RUSTFLAGS=-Clink-arg=--max-memory=268435456 cargo test --manifest-path rust/wasm/Cargo.toml --target wasm32-unknown-unknown --test engine_compaction > experiments/issue-138/engine-compaction-wasm-binding-focused.log 2>&1
printf 'Actual-WASM core and binding compaction controls passed\n'
dotnet test csharp --configuration Debug --disable-build-servers -m:1 --filter FullyQualifiedName~EngineCompact > experiments/issue-138/engine-compaction-csharp-focused.log 2>&1
printf 'C# compaction controls passed\n'
(cd js/client && node --test test/engine-compaction*.test.mjs && npm run typecheck) > experiments/issue-138/engine-compaction-js-focused.log 2>&1
printf 'JS compaction controls and strict TypeScript passed\n'
