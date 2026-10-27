#!/usr/bin/env bash
# Run in an isolated clean baseline, passing the absolute PR fixture checkout.
set -euo pipefail
fixture_checkout="${1:?provide absolute path to the PR fixture checkout}"
baseline=bc364d2e09100bc5bd0d93274b2ecd870e31b66e
if [[ "$(git rev-parse HEAD)" != "$baseline" ]] || [[ -n "$(git status --porcelain)" ]]; then
 printf 'Requires a clean isolated worktree at %s; refuses to overwrite current work.\n' "$baseline" >&2; exit 2
fi
mkdir -p experiments/issue-138
cp "$fixture_checkout/experiments/issue-138/checkpoint-inspect-before.rs" rust/tests/checkpoint_inspect_before.rs
cp "$fixture_checkout/experiments/issue-138/CheckpointInspectBeforeTests.cs" csharp/Foundation.Data.Doublets.Cli.Tests/CheckpointInspectBeforeTests.cs
cp "$fixture_checkout/experiments/issue-138/checkpoint-inspect-before.test.mjs" experiments/issue-138/checkpoint-inspect-before.test.mjs
export CARGO_BUILD_JOBS=1 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 DOTNET_GCHeapHardLimit=0x28000000
expect_missing() {
 local language="$1" pattern="$2"; shift 2
 if "$@" > "experiments/issue-138/checkpoint-inspect-$language-before.log" 2>&1; then printf 'Unexpected success: %s inspection API exists\n' "$language" >&2; exit 1; fi
 rg -q "$pattern" "experiments/issue-138/checkpoint-inspect-$language-before.log"
 printf '%s inspection missing-API control failed as expected\n' "$language"
}
expect_missing js 'inspect is not a function' node --test experiments/issue-138/checkpoint-inspect-before.test.mjs
expect_missing rust 'error\[E0599\]' cargo test --manifest-path rust/Cargo.toml --no-default-features --features native-pages --test checkpoint_inspect_before
expect_missing csharp 'error CS0117:.*CheckpointPagedCommitLog.*Inspect' dotnet test csharp --configuration Debug --disable-build-servers -m:1 --filter FullyQualifiedName~CheckpointInspectBeforeTests
