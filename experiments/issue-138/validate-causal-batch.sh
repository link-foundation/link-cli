#!/usr/bin/env bash
# One compiler at a time; finite, reaped physical-failure children.
set -euo pipefail
cd "$(dirname "$0")/../.."
export CARGO_BUILD_JOBS=1 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0
export DOTNET_GCHeapHardLimit=0x28000000
export PATH="$PWD/experiments/issue-138/tools/dotnet-10.0.403:$PWD/experiments/issue-138/tools:$PWD/experiments/issue-138/tools/wasm-bindgen-0.2.129-x86_64-unknown-linux-musl:$PATH"
prefix=experiments/issue-138/causal-batch
printf 'START %s\n' "$(date -u +%FT%TZ)"
dotnet --version > "$prefix-dotnet-sdk.log"
case "${1:-all}" in
all)
bash experiments/issue-138/validate-causal-batch-focused.sh > "$prefix-focused-driver.log" 2>&1
cargo build --manifest-path rust/Cargo.toml --no-default-features --features native-pages,physical-lsm,causal-batches \
  --example native_lsm > "$prefix-rust-example-build.log" 2>&1
dotnet build examples/cluster-csharp/ClusterExample.csproj --configuration Release --disable-build-servers -m:1 > "$prefix-csharp-example.log" 2>&1
NATIVE_LSM_LAYOUT='batch' python3 examples/native-lsm-js/run-interop.py > "$prefix-physical-handoffs.log" 2>&1
NATIVE_CAUSAL_LAYOUT='batch' python3 examples/native-lsm-js/run-causal-faults.py > "$prefix-physical-faults.log" 2>&1
printf 'PHYSICAL PASS %s\n' "$(date -u +%FT%TZ)"
;;
checks|native|compilers) ;; # Resume after independently retained prior receipts.
*) printf 'usage: %s [all|checks|native|compilers]\n' "$0" >&2; exit 2 ;;
esac
if [[ "${1:-all}" == all || "${1:-all}" == checks ]]; then
(cd js/client; node --max-old-space-size=256 --test --test-concurrency=1 --test-timeout=30000 \
  --experimental-test-coverage --test-coverage-include='src/**' \
  --test-coverage-lines=100 --test-coverage-branches=100 --test-coverage-functions=100 \
  test/*.test.mjs) > "$prefix-js-full.log" 2>&1
bun test js/client/test/engine-causal-batch.test.mjs > "$prefix-bun.log" 2>&1
deno test --allow-all --v8-flags=--max-old-space-size=128 js/client/test/engine-causal-batch.test.mjs > "$prefix-deno.log" 2>&1
printf 'JAVASCRIPT PASS %s\n' "$(date -u +%FT%TZ)"
fi
if [[ "${1:-all}" != compilers ]]; then
dotnet format csharp --no-restore --verify-no-changes > "$prefix-csharp-format.log" 2>&1
dotnet format examples/cluster-csharp/ClusterExample.csproj --no-restore --verify-no-changes >> "$prefix-csharp-format.log" 2>&1
dotnet test csharp/Foundation.Data.Doublets.Cli.Tests --no-restore --filter FullyQualifiedName~CausalBatchTests \
  --collect:'XPlat Code Coverage' --results-directory "$prefix-csharp-focused-coverage" > "$prefix-csharp-focused.log" 2>&1
fi
dotnet build-server shutdown --vbcscompiler > "$prefix-build-server-shutdown.log" 2>&1
cargo clippy --manifest-path rust/Cargo.toml --all-features --all-targets -- -D warnings > "$prefix-native-clippy.log" 2>&1
cargo clippy --manifest-path rust/wasm/Cargo.toml --target wasm32-unknown-unknown --all-targets -- -D warnings > "$prefix-wasm-clippy.log" 2>&1
cargo tree --manifest-path rust/Cargo.toml --no-default-features --edges normal > "$prefix-bare-dependencies.log" 2>&1
cargo tree --manifest-path rust/Cargo.toml --no-default-features --features causal-batches --edges normal > "$prefix-selected-dependencies.log" 2>&1
if rg -q 'serde_json|rusqlite|sha2' "$prefix-bare-dependencies.log" || rg -q 'rusqlite|sha2' "$prefix-selected-dependencies.log"; then
  printf 'selected dependency boundary changed\n' >&2; exit 1
fi
for mode in Debug Release; do
  dotnet test csharp --configuration "$mode" --disable-build-servers -m:1 \
    --collect:'XPlat Code Coverage' --results-directory "$PWD/$prefix-$mode-results" > "$prefix-csharp-$mode-full.log" 2>&1
done
printf 'CSHARP PASS %s\n' "$(date -u +%FT%TZ)"
RUSTC_BOOTSTRAP=1 cargo llvm-cov test --manifest-path rust/Cargo.toml --all-features \
  --branch --json --output-path "$prefix-native-coverage.json" > "$prefix-native-full.log" 2>&1
cargo test --manifest-path rust/Cargo.toml --all-features --doc > "$prefix-doctests.log" 2>&1
printf 'NATIVE PASS %s\n' "$(date -u +%FT%TZ)"
WASM_COVERAGE_EXTRA_RUSTFLAGS=-Clink-arg=--max-memory=268435456 \
  CARGO_LLVM_COV_BIN="$PWD/experiments/issue-138/tools/cargo-llvm-cov" \
  bash experiments/issue-138/wasm-coverage.sh > "$prefix-wasm-full.log" 2>&1
cp experiments/issue-138/wasm-coverage.json "$prefix-wasm-coverage.json"
printf 'ACTUAL WASM PASS %s\n' "$(date -u +%FT%TZ)"
npm run test:js --prefix js > "$prefix-repository.log" 2>&1
node csharp/scripts/check-file-size.mjs > "$prefix-source-size.log" 2>&1
node csharp/scripts/validate-changeset.mjs > "$prefix-changeset.log" 2>&1
GITHUB_TOKEN="$(gh auth token)" node .github/scripts/check-latest-dependencies.mjs > "$prefix-dependency-freshness.log" 2>&1
PATH="$PWD/experiments/issue-138/tools/shellcheck-v0.11.0:$PATH" experiments/issue-138/tools/actionlint > "$prefix-workflows.log" 2>&1
experiments/issue-138/tools/shellcheck-v0.11.0/shellcheck experiments/issue-138/validate-causal-batch{,-focused,-csharp,-admission}.sh >> "$prefix-workflows.log" 2>&1
git diff --check > "$prefix-whitespace.log" 2>&1
printf 'ALL PASS %s\n' "$(date -u +%FT%TZ)"
