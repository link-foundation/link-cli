#!/usr/bin/env bash
# Run focused validation immediately before this script: full C# uses --no-build.
# Requires restored dependencies, Node/.NET and setup-resumed-validation.sh tools.
set -euo pipefail
cd "$(dirname "$0")/../.."
export CARGO_BUILD_JOBS=1 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0
export DOTNET_GCHeapHardLimit=0x28000000
export PATH="$PWD/experiments/issue-138/tools:$PWD/experiments/issue-138/tools/wasm-bindgen-0.2.129-x86_64-unknown-linux-musl:$PATH"
export CARGO_TARGET_WASM32_UNKNOWN_UNKNOWN_RUNNER=wasm-bindgen-test-runner WASM_BINDGEN_TEST_TIMEOUT=30
cargo fmt --manifest-path rust/Cargo.toml --check
cargo fmt --manifest-path rust/wasm/Cargo.toml --check
dotnet format csharp --no-restore --verify-no-changes > experiments/issue-138/resumed-ancestry-format.log 2>&1
node csharp/scripts/validate-changeset.mjs > experiments/issue-138/resumed-ancestry-changeset.log 2>&1
node csharp/scripts/check-file-size.mjs --lang all > experiments/issue-138/resumed-ancestry-file-size.log 2>&1
printf 'Formatting, changesets, source sizes: passed\n'
for mode in Debug Release; do
 dotnet test csharp --configuration "$mode" --no-build --no-restore --disable-build-servers -m:1 --collect:'XPlat Code Coverage' --results-directory "$PWD/experiments/issue-138/resumed-ancestry-$mode-results" > "experiments/issue-138/resumed-ancestry-csharp-$mode-full.log" 2>&1
 printf 'Full C# %s with coverage: passed\n' "$mode"
done
cargo clippy --manifest-path rust/Cargo.toml --all-features --all-targets -- -D warnings > experiments/issue-138/resumed-ancestry-clippy.log 2>&1
cargo clippy --manifest-path rust/wasm/Cargo.toml --target wasm32-unknown-unknown --all-targets -- -D warnings > experiments/issue-138/resumed-ancestry-wasm-clippy.log 2>&1
printf 'Strict native and WASM Clippy: passed\n'
RUSTC_BOOTSTRAP=1 cargo llvm-cov test --manifest-path rust/Cargo.toml --all-features --branch --json --output-path experiments/issue-138/resumed-ancestry-native-coverage.json > experiments/issue-138/resumed-ancestry-native-full.log 2>&1
printf 'Full native Rust coverage suite: passed\n'
CARGO_LLVM_COV_BIN="$PWD/experiments/issue-138/tools/cargo-llvm-cov" bash experiments/issue-138/wasm-coverage.sh > experiments/issue-138/resumed-ancestry-wasm-full.log 2>&1
cp experiments/issue-138/wasm-coverage.json experiments/issue-138/resumed-ancestry-wasm-coverage.json
printf 'Actual WASM coverage suite: passed\n'
(cd js/client && npm ci && node --test --test-concurrency=1 --test-timeout=30000 --experimental-test-coverage --test-coverage-include='src/**' --test-coverage-lines=100 --test-coverage-branches=100 --test-coverage-functions=100 test/*.test.mjs && npm run typecheck) > experiments/issue-138/resumed-ancestry-js-full.log 2>&1
node --test .github/scripts/*.test.mjs js/test/*.test.mjs csharp/scripts/*.test.mjs > experiments/issue-138/resumed-ancestry-repository-tests.log 2>&1
git diff --check
printf 'JS coverage, types, repository tests and whitespace: passed\n'
printf 'All resumed ancestry local checks: passed\n'
