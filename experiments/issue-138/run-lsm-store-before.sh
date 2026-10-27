#!/usr/bin/env bash
# Reproduce against an isolated clean codec-only worktree.
set -euo pipefail
fixture_checkout="${1:?provide absolute path to the PR fixture checkout}"
baseline=d12760fc13db30439d114ee15ab98be970aae5f1
if [[ "$(git rev-parse HEAD)" != "$baseline" ]] || [[ -n "$(git status --porcelain)" ]]; then
 printf 'Requires a clean isolated worktree at %s; refuses to overwrite current work.\n' "$baseline" >&2; exit 2
fi
mkdir -p experiments/issue-138/lsm-store-before-rust experiments/issue-138/lsm-store-before-csharp
for name in lsm-store-before.rs LsmStoreBeforeTests.cs lsm-store-before.test.mjs lsm-store-harness.mjs run-lsm-store-before-current.sh; do
 cp "$fixture_checkout/experiments/issue-138/$name" "experiments/issue-138/$name"
done
cp "$fixture_checkout/experiments/issue-138/lsm-store-before-rust/Cargo.toml" experiments/issue-138/lsm-store-before-rust/Cargo.toml
cp "$fixture_checkout/experiments/issue-138/lsm-store-before-csharp/Before.csproj" experiments/issue-138/lsm-store-before-csharp/Before.csproj
export PATH="$fixture_checkout/experiments/issue-138/tools/dotnet-10.0.403:$fixture_checkout/experiments/issue-138/tools:$PATH"
bash experiments/issue-138/run-lsm-store-before-current.sh
