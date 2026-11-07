#!/usr/bin/env bash
# Complete unfiltered suites, one compiler at a time, with immutable report identities.
set -euo pipefail
cd "$(dirname "$0")/../.."
export CARGO_BUILD_JOBS=1 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0
export DOTNET_GCHeapHardLimit=0x28000000
export PATH="$PWD/experiments/issue-138/tools/dotnet-10.0.403:$PWD/experiments/issue-138/tools:$PWD/experiments/issue-138/tools/wasm-bindgen-0.2.129-x86_64-unknown-linux-musl:$PATH"
prefix=experiments/issue-138/causal-session
printf 'START %s\n' "$(date -u +%FT%TZ)"
python3 experiments/issue-138/causal-session-model.py > "$prefix-model.log" 2>&1
python3 experiments/issue-138/generate-causal-session-vectors.py "$prefix-vectors-regenerated.json" > "$prefix-vectors.log" 2>&1
cmp docs/protocol/causal-session-v1.json "$prefix-vectors-regenerated.json"
python3 experiments/issue-138/test-causal-raw-coverage.py > "$prefix-raw-verifier.log" 2>&1
cargo fmt --manifest-path rust/Cargo.toml --check > "$prefix-format.log" 2>&1
cargo fmt --manifest-path rust/wasm/Cargo.toml --check >> "$prefix-format.log" 2>&1
cargo clippy --manifest-path rust/Cargo.toml --all-features --all-targets -- -D warnings > "$prefix-native-clippy.log" 2>&1
cargo clippy --manifest-path rust/wasm/Cargo.toml --target wasm32-unknown-unknown --all-targets -- -D warnings > "$prefix-wasm-clippy.log" 2>&1
(cd js/client; node --max-old-space-size=256 --test --test-concurrency=1 --test-timeout=30000 \
  --experimental-test-coverage --test-coverage-include='src/**' \
  --test-coverage-lines=100 --test-coverage-branches=100 --test-coverage-functions=100 \
  test/*.test.mjs) > "$prefix-js-full.log" 2>&1
npm run typecheck --prefix js/client > "$prefix-types.log" 2>&1
printf 'JAVASCRIPT PASS %s\n' "$(date -u +%FT%TZ)"
dotnet --version > "$prefix-dotnet-sdk.log"
dotnet format csharp --no-restore --verify-no-changes > "$prefix-csharp-format.log" 2>&1
for mode in Debug Release; do
  dotnet test csharp --configuration "$mode" --disable-build-servers -m:1 \
    --collect:'XPlat Code Coverage;Format=json,cobertura' --results-directory "$PWD/$prefix-$mode-results" > "$prefix-csharp-$mode-full.log" 2>&1
  report=$(find "$prefix-$mode-results" -name coverage.json -type f)
  python3 experiments/issue-138/check-causal-raw-coverage.py "$report" --require-session > "$prefix-$mode-records.log" 2>&1
done
printf 'CSHARP PASS %s\n' "$(date -u +%FT%TZ)"
dotnet build-server shutdown --vbcscompiler > "$prefix-build-server-shutdown.log" 2>&1
cargo tree --manifest-path rust/Cargo.toml --no-default-features --edges normal > "$prefix-bare-dependencies.log" 2>&1
cargo tree --manifest-path rust/Cargo.toml --no-default-features --features causal-sessions --edges normal > "$prefix-selected-dependencies.log" 2>&1
if rg -q 'serde_json|rusqlite|sha2' "$prefix-bare-dependencies.log" || rg -q 'rusqlite|sha2' "$prefix-selected-dependencies.log"; then
  printf 'selected dependency boundary changed\n' >&2; exit 1
fi
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
experiments/issue-138/tools/shellcheck-v0.11.0/shellcheck experiments/issue-138/validate-causal-session{,-actual}.sh >> "$prefix-workflows.log" 2>&1
git diff --check > "$prefix-whitespace.log" 2>&1
printf 'ALL PASS %s\n' "$(date -u +%FT%TZ)"
