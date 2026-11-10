#!/usr/bin/env bash
# Sequential compilers and bounded managed heaps for the 2.92 GiB task container.
set -euo pipefail
cd "$(dirname "$0")/../.."
export CARGO_BUILD_JOBS=1 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0
export DOTNET_ROOT="$PWD/experiments/issue-138/tools/dotnet-10.0.403"
export DOTNET_GCHeapHardLimit=0x28000000
export PATH="$DOTNET_ROOT:$PWD/experiments/issue-138/tools:$PWD/experiments/issue-138/tools/wasm-bindgen-0.2.129-x86_64-unknown-linux-musl:$PATH"
prefix=experiments/issue-138/causal-merkle
printf 'START %s\n' "$(date -u +%FT%TZ)"
python3 experiments/issue-138/generate-causal-merkle-vectors.py > "$prefix-fixtures.log"
(cd js/client && node --max-old-space-size=128 --test --test-concurrency=1 \
  --experimental-test-coverage --test-coverage-include='src/**' \
  --test-coverage-lines=100 --test-coverage-branches=100 --test-coverage-functions=100 \
  test/*.test.mjs) > "$prefix-js-full.log" 2>&1
node --max-old-space-size=128 --test --experimental-test-coverage \
  --test-coverage-include='js/client/src/engine-causal-merkle.js' \
  --test-coverage-lines=100 --test-coverage-branches=100 --test-coverage-functions=100 \
  js/client/test/engine-causal-merkle.test.mjs > "$prefix-js-focused.log" 2>&1
bun test js/client/test/engine-causal-*.test.mjs --timeout 30000 > "$prefix-bun.log" 2>&1
deno test --allow-all --v8-flags=--max-old-space-size=128 js/client/test/engine-causal-*.test.mjs > "$prefix-deno.log" 2>&1
npm run typecheck --prefix js/client > "$prefix-types.log" 2>&1
cargo fmt --manifest-path rust/Cargo.toml --all -- --check > "$prefix-format.log" 2>&1
cargo fmt --manifest-path rust/wasm/Cargo.toml --all -- --check >> "$prefix-format.log" 2>&1
dotnet format csharp --no-restore --verify-no-changes > "$prefix-csharp-format.log" 2>&1
dotnet format examples/cluster-csharp/ClusterExample.csproj --no-restore --verify-no-changes >> "$prefix-csharp-format.log" 2>&1
dotnet build examples/cluster-csharp/ClusterExample.csproj --configuration Release --disable-build-servers -m:1 > "$prefix-csharp-example.log" 2>&1
dotnet test csharp/Foundation.Data.Doublets.Cli.Tests --configuration Debug --disable-build-servers -m:1 \
  --filter 'FullyQualifiedName~CausalVersionTests|FullyQualifiedName~CausalReplicaTests|FullyQualifiedName~CausalMerkleTests' > "$prefix-csharp-focused.log" 2>&1
cargo test --manifest-path rust/Cargo.toml --all-features --test engine_causal_merkle_tests > "$prefix-rust-focused.log" 2>&1
cargo build --manifest-path rust/Cargo.toml --no-default-features --features native-pages,physical-lsm,causal-merkle \
  --example native_lsm > "$prefix-rust-example.log" 2>&1
python3 examples/native-lsm-js/run-merkle-interop.py > "$prefix-physical-handoffs.log" 2>&1
printf 'PHYSICAL COMPOSITION PASS %s\n' "$(date -u +%FT%TZ)"
export CARGO_TARGET_WASM32_UNKNOWN_UNKNOWN_RUNNER=wasm-bindgen-test-runner
WASM_BINDGEN_TEST_TIMEOUT=30 cargo test --manifest-path rust/wasm/Cargo.toml \
  --target wasm32-unknown-unknown --test core causal_merkle > "$prefix-wasm-focused.log" 2>&1
cargo clippy --manifest-path rust/Cargo.toml --all-features --all-targets -- -D warnings > "$prefix-rust-clippy.log" 2>&1
cargo clippy --manifest-path rust/wasm/Cargo.toml --target wasm32-unknown-unknown --all-targets -- -D warnings > "$prefix-wasm-clippy.log" 2>&1
cargo tree --manifest-path rust/Cargo.toml --no-default-features --edges normal > "$prefix-bare-dependencies.log" 2>&1
if rg -q 'serde_json|rusqlite|sha2' "$prefix-bare-dependencies.log"; then
  printf 'bare dependency boundary changed\n' >&2; exit 1
fi
node csharp/scripts/check-file-size.mjs > "$prefix-source-sizes.log" 2>&1
node csharp/scripts/validate-changeset.mjs > "$prefix-changeset.log" 2>&1
printf 'STRICT AND FOCUSED PASS %s\n' "$(date -u +%FT%TZ)"
for mode in Debug Release; do
  dotnet test csharp --configuration "$mode" --disable-build-servers -m:1 \
    --collect:'XPlat Code Coverage' --results-directory "$PWD/$prefix-$mode-results" \
    > "$prefix-csharp-$mode-full.log" 2>&1
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
GITHUB_TOKEN="$(gh auth token)" node .github/scripts/check-latest-dependencies.mjs > "$prefix-dependency-freshness.log" 2>&1
PATH="$PWD/experiments/issue-138/tools/shellcheck-v0.11.0:$PATH" experiments/issue-138/tools/actionlint > "$prefix-workflows.log" 2>&1
experiments/issue-138/tools/shellcheck-v0.11.0/shellcheck experiments/issue-138/validate-causal-merkle.sh >> "$prefix-workflows.log" 2>&1
git diff --check > "$prefix-whitespace.log" 2>&1
printf 'ALL PASS %s\n' "$(date -u +%FT%TZ)"
