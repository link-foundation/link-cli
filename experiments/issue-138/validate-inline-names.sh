#!/usr/bin/env bash
# Run sequential compilers and bounded graph contracts within the container.
set -euo pipefail
cd "$(dirname "$0")/../.."
export CARGO_BUILD_JOBS=1 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0
export DOTNET_GCHeapHardLimit=0x28000000
export PATH="$PWD/experiments/issue-138/tools:$PWD/experiments/issue-138/tools/wasm-bindgen-0.2.129-x86_64-unknown-linux-musl:$PATH"
export CARGO_TARGET_WASM32_UNKNOWN_UNKNOWN_RUNNER=wasm-bindgen-test-runner WASM_BINDGEN_TEST_TIMEOUT=30 RUST_LOG=warn
cargo fmt --manifest-path rust/Cargo.toml
cargo fmt --manifest-path rust/wasm/Cargo.toml
dotnet format csharp --no-restore > experiments/issue-138/inline-names-format.log 2>&1
node csharp/scripts/validate-changeset.mjs > experiments/issue-138/inline-names-changeset.log 2>&1
node csharp/scripts/check-file-size.mjs --lang all > experiments/issue-138/inline-names-file-size.log 2>&1
printf 'Formatting, release fragments and source sizes: passed\n'
if [[ "${INLINE_NAMES_RESUME_AFTER_CSHARP:-0}" != 1 ]]; then
 for mode in Debug Release; do
  dotnet test csharp --configuration "$mode" --disable-build-servers -m:1 --collect:'XPlat Code Coverage' --results-directory "$PWD/experiments/issue-138/inline-names-$mode-results" > "experiments/issue-138/inline-names-csharp-$mode-full.log" 2>&1
  printf 'Full C# %s with coverage: passed\n' "$mode"
 done
fi
cargo clippy --manifest-path rust/Cargo.toml --all-features --all-targets -- -D warnings > experiments/issue-138/inline-names-clippy.log 2>&1
cargo clippy --manifest-path rust/wasm/Cargo.toml --target wasm32-unknown-unknown --all-targets -- -D warnings > experiments/issue-138/inline-names-wasm-clippy.log 2>&1
printf 'Strict native and WASM Clippy: passed\n'
RUSTC_BOOTSTRAP=1 cargo llvm-cov test --manifest-path rust/Cargo.toml --all-features --branch --json --output-path experiments/issue-138/inline-names-native-coverage.json > experiments/issue-138/inline-names-native-full.log 2>&1
printf 'Full native Rust with optional backends and coverage: passed\n'
WASM_COVERAGE_EXTRA_RUSTFLAGS=-Clink-arg=--max-memory=268435456 CARGO_LLVM_COV_BIN="$PWD/experiments/issue-138/tools/cargo-llvm-cov" bash experiments/issue-138/wasm-coverage.sh > experiments/issue-138/inline-names-wasm-full.log 2>&1
cp experiments/issue-138/wasm-coverage.json experiments/issue-138/inline-names-wasm-coverage.json
python3 experiments/issue-138/verify-wasm-memory-cap.py > experiments/issue-138/inline-names-wasm-memory-cap.log
printf 'Actual WASM full coverage suite: passed\n'
(cd js/client && node --test --test-concurrency=1 --test-timeout=30000 --experimental-test-coverage --test-coverage-include='src/**' --test-coverage-lines=100 --test-coverage-branches=100 --test-coverage-functions=100 test/*.test.mjs && npm run typecheck) > experiments/issue-138/inline-names-js-full.log 2>&1
node --test .github/scripts/*.test.mjs js/test/*.test.mjs csharp/scripts/*.test.mjs > experiments/issue-138/inline-names-repository-tests.log 2>&1
"$PWD/experiments/issue-138/tools/shellcheck-v0.11.0/shellcheck" experiments/issue-138/validate-inline-names.sh experiments/issue-138/setup-resumed-validation.sh > experiments/issue-138/inline-names-shellcheck.log 2>&1
actionlint > experiments/issue-138/inline-names-actionlint.log 2>&1
git diff --check
printf 'JS 100%% coverage, TypeScript and repository checks: passed\n'
cargo build --manifest-path rust/wasm/Cargo.toml --target wasm32-unknown-unknown > experiments/issue-138/inline-names-wasm-package.log 2>&1
wasm-bindgen --target web --out-dir experiments/issue-138/inline-names-wasm-package rust/wasm/target/wasm32-unknown-unknown/debug/clink_wasm.wasm >> experiments/issue-138/inline-names-wasm-package.log 2>&1
export CLINK_WASM="$PWD/experiments/issue-138/inline-names-wasm-package"
for runtime in node bun; do
 "$runtime" js/client/test/conformance/run.mjs > "experiments/issue-138/inline-names-$runtime-conformance.log" 2>&1
done
deno run --allow-all js/client/test/conformance/run.mjs > experiments/issue-138/inline-names-deno-conformance.log 2>&1
printf 'Node, Bun and Deno actual-WASM conformance: passed\n'
