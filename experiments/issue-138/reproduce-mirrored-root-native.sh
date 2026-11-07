#!/usr/bin/env bash
# Copy saved fixtures into a clean isolated baseline; never overwrite the fix.
set -euo pipefail
cd "$(dirname "$0")/../.."
baseline=6a0f7c96850ca6b507f924fcfcbe429357b32864
if [[ "$(git rev-parse HEAD)" != "$baseline" ]] || [[ -n "$(git status --porcelain)" ]]; then
 printf '%s\n' "Requires a clean isolated worktree at $baseline; refusing to overwrite current tests." >&2
 exit 2
fi
export CARGO_BUILD_JOBS=1 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0
export DOTNET_GCHeapHardLimit=0x28000000
export PATH="$PWD/experiments/issue-138/tools/dotnet-10.0.403:$PATH"
cp experiments/issue-138/mirrored-root-native-before.rs rust/tests/engine_file_checkpoint_root_tests.rs
cp experiments/issue-138/NativeCheckpointRootBeforeTests.cs csharp/Foundation.Data.Doublets.Cli.Tests/NativeCheckpointRootTests.cs
cp experiments/issue-138/mirrored-root-native-before.test.mjs js/client/test/engine-file-checkpoint-root.test.mjs
expect_missing() {
 local language="$1" pattern="$2"
 shift 2
 if "$@" > "experiments/issue-138/mirrored-root-native-$language-before.log" 2>&1; then
  printf '%s\n' "Unexpected success in $language native root control." >&2
  exit 1
 fi
 rg -q "$pattern" "experiments/issue-138/mirrored-root-native-$language-before.log"
 printf '%s\n' "$language native mirrored-root control: failed as expected"
}
expect_missing rust 'error\[E0432\].*FileCheckpointOptions' cargo test --manifest-path rust/Cargo.toml --no-default-features --features native-pages --test engine_file_checkpoint_root_tests
expect_missing csharp 'error CS1739:.*mirroredRoot' dotnet test csharp --configuration Debug --disable-build-servers -m:1 --filter FullyQualifiedName~NativeCheckpointRootTests
expect_missing js 'false !== true' node --test --test-timeout=30000 js/client/test/engine-file-checkpoint-root.test.mjs
