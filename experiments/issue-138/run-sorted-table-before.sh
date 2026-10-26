#!/usr/bin/env bash
# Run from an isolated clean baseline worktree, passing the fixture checkout.
set -euo pipefail
fixture_checkout="${1:?provide absolute path to the PR fixture checkout}"
baseline=9a9a95cda375395a54005a5f6a8495e9eee2fde9
if [[ "$(git rev-parse HEAD)" != "$baseline" ]] || [[ -n "$(git status --porcelain)" ]]; then
 printf 'Requires a clean isolated worktree at %s; refuses to overwrite current work.\n' "$baseline" >&2;exit 2
fi
mkdir -p experiments/issue-138 rust/tests/engine_paged js/client/test/support
cp "$fixture_checkout/experiments/issue-138/sorted-table-before.rs" rust/tests/engine_paged/sorted_table.rs
printf '\n#[path = "engine_paged/sorted_table.rs"]\nmod sorted_table;\n' >> rust/tests/engine_paged_tests.rs
cp "$fixture_checkout/experiments/issue-138/SortedTableBeforeTests.cs" csharp/Foundation.Data.Doublets.Cli.Tests/SortedTableBeforeTests.cs
cp "$fixture_checkout/experiments/issue-138/sorted-table-before.test.mjs" js/client/test/engine-sorted-table.test.mjs
cp "$fixture_checkout/js/client/test/support/sorted-table.mjs" js/client/test/support/sorted-table.mjs
export CARGO_BUILD_JOBS=1 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 DOTNET_GCHeapHardLimit=0x28000000
expect_missing() {
 local language="$1" pattern="$2";shift 2
 if "$@" > "experiments/issue-138/sorted-table-$language-before.log" 2>&1;then printf 'Unexpected success: %s table API exists\n' "$language" >&2;exit 1;fi
 rg -q "$pattern" "experiments/issue-138/sorted-table-$language-before.log"
 printf '%s table missing-API control failed as expected\n' "$language"
}
expect_missing js ERR_MODULE_NOT_FOUND node --test js/client/test/engine-sorted-table.test.mjs
expect_missing rust 'unresolved import.*SortedJournalEntry' cargo test --manifest-path rust/Cargo.toml --all-features --test engine_paged_tests
expect_missing csharp 'error CS0246:.*SortedJournalEntry' dotnet test csharp --configuration Debug --disable-build-servers -m:1 --filter FullyQualifiedName~SortedJournalTableTests
