#!/usr/bin/env bash
# Copy saved minimal fixtures into a clean isolated baseline checkout.
set -euo pipefail
cd "$(dirname "$0")/../.."
baseline=44e0fa889e291db571b7e987cc1b7e06764f8ae7
if [[ "$(git rev-parse HEAD)" != "$baseline" ]] || [[ -n "$(git status --porcelain)" ]]; then
 printf '%s\n' "Requires a clean isolated worktree at $baseline; refusing to overwrite current tests." >&2
 exit 2
fi
cp experiments/issue-138/reclamation-before.rs rust/tests/engine_reclaiming_tests.rs
cp experiments/issue-138/ReclamationBeforeTests.cs csharp/Foundation.Data.Doublets.Cli.Tests/ReclamationBeforeTests.cs
cp experiments/issue-138/reclamation-initial.test.mjs js/client/test/reclaiming.test.mjs
export CARGO_BUILD_JOBS=1 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 DOTNET_GCHeapHardLimit=0x28000000
export PATH="$PWD/experiments/issue-138/tools/dotnet-10.0.403:$PATH"
expect_missing() {
 local language="$1" pattern="$2"
 shift 2
 if "$@" > "experiments/issue-138/reclamation-$language-before.log" 2>&1; then
  printf '%s\n' "Unexpected success: $language collector API exists." >&2; exit 1
 fi
 rg -q "$pattern" "experiments/issue-138/reclamation-$language-before.log"
 printf '%s\n' "$language collector missing-API control failed as expected"
}
expect_missing js 'does not provide an export named.*ReclaimingPartitionedJournalStore' node --test --test-timeout=10000 js/client/test/reclaiming.test.mjs
expect_missing rust 'unresolved import.*reclaiming' cargo test --manifest-path rust/Cargo.toml --all-features --test engine_reclaiming_tests
expect_missing csharp 'error CS0103:.*ReclaimingPartitionedJournalStore' dotnet test csharp --configuration Debug --disable-build-servers -m:1 --filter FullyQualifiedName~ReclamationBeforeTests
