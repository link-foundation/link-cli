#!/usr/bin/env bash
# No deliberate pressure probes; one compiler and the established finite suites.
set -euo pipefail
cd "$(dirname "$0")/../.."
export CARGO_BUILD_JOBS=1 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0
export PATH="$PWD/experiments/issue-138/tools:$PWD/experiments/issue-138/tools/wasm-bindgen-0.2.129-x86_64-unknown-linux-musl:$PATH"
log_prefix=experiments/issue-138/merkle-admission
printf 'START %s\n' "$(date -u +%FT%TZ)"
cargo fmt --manifest-path rust/Cargo.toml --all -- --check > "$log_prefix-format.log" 2>&1
cargo fmt --manifest-path rust/wasm/Cargo.toml --all -- --check >> "$log_prefix-format.log" 2>&1
cargo test --manifest-path rust/Cargo.toml --all-features --test engine_causal_merkle_tests > "$log_prefix-native-focused.log" 2>&1
CARGO_TARGET_WASM32_UNKNOWN_UNKNOWN_RUNNER=wasm-bindgen-test-runner WASM_BINDGEN_TEST_TIMEOUT=30 \
  cargo test --manifest-path rust/wasm/Cargo.toml --target wasm32-unknown-unknown --test core causal_merkle \
  > "$log_prefix-wasm-focused.log" 2>&1
printf 'FOCUSED PASS %s\n' "$(date -u +%FT%TZ)"
cargo clippy --manifest-path rust/Cargo.toml --all-features --all-targets -- -D warnings > "$log_prefix-native-clippy.log" 2>&1
cargo clippy --manifest-path rust/wasm/Cargo.toml --target wasm32-unknown-unknown --all-targets -- -D warnings > "$log_prefix-wasm-clippy.log" 2>&1
RUSTC_BOOTSTRAP=1 cargo llvm-cov test --manifest-path rust/Cargo.toml --all-features \
  --branch --json --output-path "$log_prefix-native-coverage.json" > "$log_prefix-native-full.log" 2>&1
cargo test --manifest-path rust/Cargo.toml --all-features --doc > "$log_prefix-doctests.log" 2>&1
printf 'NATIVE PASS %s\n' "$(date -u +%FT%TZ)"
WASM_COVERAGE_EXTRA_RUSTFLAGS=-Clink-arg=--max-memory=268435456 \
  CARGO_LLVM_COV_BIN="$PWD/experiments/issue-138/tools/cargo-llvm-cov" \
  bash experiments/issue-138/wasm-coverage.sh > "$log_prefix-wasm-full.log" 2>&1
cp experiments/issue-138/wasm-coverage.json "$log_prefix-wasm-coverage.json"
printf 'ACTUAL WASM PASS %s\n' "$(date -u +%FT%TZ)"
python3 experiments/issue-138/summarize-merkle-admission.py --require-covered \
  "$log_prefix-native-coverage.json" "$log_prefix-wasm-coverage.json" > "$log_prefix-coverage.log"
npm run test:js --prefix js > "$log_prefix-repository.log" 2>&1
node csharp/scripts/check-file-size.mjs > "$log_prefix-source-size.log" 2>&1
node csharp/scripts/validate-changeset.mjs > "$log_prefix-changeset.log" 2>&1
PATH="$PWD/experiments/issue-138/tools/shellcheck-v0.11.0:$PATH" experiments/issue-138/tools/actionlint > "$log_prefix-workflows.log" 2>&1
experiments/issue-138/tools/shellcheck-v0.11.0/shellcheck experiments/issue-138/validate-merkle-admission.sh >> "$log_prefix-workflows.log" 2>&1
git diff --check > "$log_prefix-whitespace.log" 2>&1
printf 'ALL PASS %s\n' "$(date -u +%FT%TZ)"
