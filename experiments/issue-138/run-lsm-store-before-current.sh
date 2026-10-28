#!/usr/bin/env bash
# Nonmutating source controls against the codec-only baseline; no store exists.
set -euo pipefail
cd "$(dirname "$0")/../.."
baseline=d12760fc13db30439d114ee15ab98be970aae5f1
if [[ "$(git rev-parse HEAD)" != "$baseline" ]] || [[ -e js/client/src/engine-lsm-store.js ]] \
 || [[ -e rust/src/engine/lsm/store.rs ]] || [[ -e csharp/Foundation.Data.Doublets.Cli.Library/Engine/PhysicalLsmJournalStore.cs ]]; then
 printf 'Requires the codec-only baseline %s before store implementation.\n' "$baseline" >&2; exit 2
fi
export CARGO_BUILD_JOBS=1 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 DOTNET_GCHeapHardLimit=0x28000000
export PATH="$PWD/experiments/issue-138/tools/dotnet-10.0.403:$PWD/experiments/issue-138/tools:$PATH"
expect_missing() {
 local language="$1" pattern="$2"; shift 2
 if "$@" > "experiments/issue-138/lsm-store-before-codec-$language.log" 2>&1; then
  printf 'Unexpected success: %s physical store API exists\n' "$language" >&2; exit 1
 fi
 rg -q "$pattern" "experiments/issue-138/lsm-store-before-codec-$language.log"
 printf '%s physical store API fails as expected %s\n' "$language" "$(date -u +%FT%TZ)"
}
expect_missing js ERR_MODULE_NOT_FOUND node --max-old-space-size=128 --test experiments/issue-138/lsm-store-before.test.mjs
CARGO_TARGET_DIR="$PWD/rust/target" expect_missing rust 'unresolved imports.*LsmStoreOptions' cargo check --manifest-path experiments/issue-138/lsm-store-before-rust/Cargo.toml --tests
expect_missing csharp 'error CS0246:.*ILsmTableNamespace' dotnet test experiments/issue-138/lsm-store-before-csharp/Before.csproj --configuration Debug --disable-build-servers -m:1
