#!/usr/bin/env bash
# Reproduce all three missing native APIs on an isolated immutable source archive.
set -euo pipefail
cd "$(dirname "$0")/../.."
export CARGO_BUILD_JOBS=1 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 DOTNET_GCHeapHardLimit=0x28000000
prefix=experiments/issue-138/native-lsm-before-baseline
baseline=experiments/issue-138/native-lsm-baseline
if [[ ! -d "$baseline/rust" ]]; then
 mkdir -p "$baseline"
 git archive bc364d2e09100bc5bd0d93274b2ecd870e31b66e rust csharp js/client | tar -x -C "$baseline"
fi
for pair in 'rust Cargo.toml' 'csharp Before.csproj'; do
 read -r language manifest <<< "$pair"
 mkdir -p "$prefix-$language"
 cp "$prefix-$language/$manifest.fixture" "$prefix-$language/$manifest"
done
expect_failure() {
 local label=$1 pattern=$2
 shift 2
 if "$@" > "$prefix-$label.log" 2>&1; then
  printf 'Unexpected successful baseline: %s\n' "$label" >&2; exit 1
 fi
 rg -q "$pattern" "$prefix-$label.log"
}
expect_failure js ERR_MODULE_NOT_FOUND node --test --test-concurrency=1 --test-timeout=30000 "$prefix.test.mjs"
expect_failure rust 'unresolved imports.*FileLsmJournalStore' cargo test --manifest-path "$prefix-rust/Cargo.toml" --target-dir "$PWD/rust/target" --test native_lsm_before
expect_failure csharp 'error CS(0246|0103).*FileLsm' dotnet test "$prefix-csharp/Before.csproj" --disable-build-servers -m:1
printf 'All three absent native APIs reproduced against clean bc364d2 source.\n'
