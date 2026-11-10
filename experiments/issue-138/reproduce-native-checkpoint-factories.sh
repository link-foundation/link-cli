#!/usr/bin/env bash
# Copy the minimal fixtures to actual test targets before implementing factories.
# Run in an isolated clean worktree at the preceding compaction commit.
# This script copies minimal test fixtures; it never overwrites an implementation.
set -euo pipefail
cd "$(dirname "$0")/../.."
baseline=133c4ff4a51240cda983dc34f7e4ca58c44d1c4c
if [[ "$(git rev-parse HEAD)" != "$baseline" ]] || [[ -n "$(git status --porcelain)" ]]; then
 printf '%s\n' "Requires a clean isolated worktree at $baseline; refusing to overwrite current tests." >&2
 exit 2
fi
export CARGO_BUILD_JOBS=1 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0
export DOTNET_GCHeapHardLimit=0x28000000
export PATH="$PWD/experiments/issue-138/tools/dotnet-10.0.403:$PATH"
cp experiments/issue-138/native-checkpoint-factory-rust.rs rust/tests/engine_file_checkpoint_tests.rs
cp experiments/issue-138/NativeCheckpointFactoryTests.cs csharp/Foundation.Data.Doublets.Cli.Tests/NativeCheckpointFactoryTests.cs
cp experiments/issue-138/native-checkpoint-factory.test.mjs js/client/test/engine-file-checkpoint-pages.test.mjs
expect_missing() {
 local language="$1" pattern="$2"
 shift 2
 if "$@" > "experiments/issue-138/native-checkpoint-$language-before.log" 2>&1; then
  printf '%s\n' "Unexpected success: $language factory exists; inspect before continuing." >&2
  exit 1
 fi
 rg -q "$pattern" "experiments/issue-138/native-checkpoint-$language-before.log"
 printf '%s\n' "$language native factory missing-feature control: failed as expected"
}
expect_missing rust 'error\[E0432\].*FileCheckpointPagedLog' cargo test --manifest-path rust/Cargo.toml --no-default-features --features native-pages --test engine_file_checkpoint_tests
expect_missing csharp 'error CS0246:.*FileCheckpointPagedCommitLog' dotnet test csharp --configuration Debug --disable-build-servers -m:1 --filter FullyQualifiedName~NativeCheckpointFactoryTests
expect_missing js 'ERR_MODULE_NOT_FOUND.*engine-file-checkpoint-pages' node --test --test-timeout=30000 js/client/test/engine-file-checkpoint-pages.test.mjs
