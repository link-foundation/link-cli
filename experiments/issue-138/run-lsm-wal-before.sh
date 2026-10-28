#!/usr/bin/env bash
# Use only in an isolated clean baseline; pass the absolute PR fixture checkout.
set -euo pipefail
fixture_checkout="${1:?provide absolute path to the PR fixture checkout}"
baseline=523888a302c49d8b148ab28d69acc9ad34bdf87d
if [[ "$(git rev-parse HEAD)" != "$baseline" ]] || [[ -n "$(git status --porcelain)" ]]; then
 printf 'Requires a clean isolated worktree at %s; refuses to overwrite current work.\n' "$baseline" >&2;exit 2
fi
mkdir -p experiments/issue-138
cp "$fixture_checkout/experiments/issue-138/lsm-wal-before.rs" rust/tests/engine_lsm_wal_tests.rs
cp "$fixture_checkout/experiments/issue-138/LsmWalBeforeTests.cs" csharp/Foundation.Data.Doublets.Cli.Tests/LsmWalBeforeTests.cs
cp "$fixture_checkout/experiments/issue-138/lsm-wal-before.test.mjs" js/client/test/engine-lsm-wal.test.mjs
export CARGO_BUILD_JOBS=1 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 DOTNET_GCHeapHardLimit=0x28000000
expect_missing() {
 local language="$1" pattern="$2";shift 2
 if "$@" > "experiments/issue-138/lsm-wal-$language-before.log" 2>&1;then printf 'Unexpected success: %s WAL API exists\n' "$language" >&2;exit 1;fi
 rg -q "$pattern" "experiments/issue-138/lsm-wal-$language-before.log"
 printf '%s WAL missing-API control failed as expected\n' "$language"
}
expect_missing js ERR_MODULE_NOT_FOUND node --test js/client/test/engine-lsm-wal.test.mjs
expect_missing rust 'unresolved import.*engine::lsm' cargo test --manifest-path rust/Cargo.toml --no-default-features --test engine_lsm_wal_tests
expect_missing csharp 'error CS0246:.*LsmOptions' dotnet test csharp --configuration Debug --disable-build-servers -m:1 --filter FullyQualifiedName~LsmWalTests
