#!/usr/bin/env bash
# Copy these saved fixtures into a clean isolated baseline checkout before running.
set -euo pipefail
cd "$(dirname "$0")/../.."
baseline=df6e01cbdf08bd11470aac8a72ef1c57a5ca927d
if [[ "$(git rev-parse HEAD)" != "$baseline" ]] || [[ -n "$(git status --porcelain)" ]]; then
 printf '%s\n' "Requires a clean isolated worktree at $baseline; refusing to overwrite current tests." >&2
 exit 2
fi
export CARGO_BUILD_JOBS=1 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 DOTNET_GCHeapHardLimit=0x28000000
export PATH="$PWD/experiments/issue-138/tools/dotnet-10.0.403:$PATH"
cp experiments/issue-138/maintenance-before.rs rust/tests/engine_etcd_maintenance_tests.rs
cp experiments/issue-138/MaintenanceBeforeTests.cs csharp/Foundation.Data.Doublets.Cli.Tests/MaintenanceBeforeTests.cs
cp experiments/issue-138/maintenance-before.test.mjs js/client/test/cluster-maintenance.test.mjs
expect_missing() {
 local language="$1" pattern="$2"
 shift 2
 if "$@" > "experiments/issue-138/maintenance-$language-before.log" 2>&1; then
  printf '%s\n' "Unexpected success: $language maintenance API already exists." >&2
  exit 1
 fi
 rg -q "$pattern" "experiments/issue-138/maintenance-$language-before.log"
 printf '%s\n' "$language maintenance missing-feature control: failed as expected"
}
expect_missing rust 'error\[E0432\].*engine::maintenance' cargo test --manifest-path rust/Cargo.toml --all-features --test engine_etcd_maintenance_tests
expect_missing csharp 'error CS0246:.*IJournalMaintenanceStore' dotnet test csharp --configuration Debug --disable-build-servers -m:1 --filter FullyQualifiedName~MaintenanceBeforeTests
expect_missing js 'scanNamespace is not a function' node --test --test-timeout=30000 js/client/test/cluster-maintenance.test.mjs
