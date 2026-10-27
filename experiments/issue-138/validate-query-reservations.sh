#!/usr/bin/env bash
# Complete native/WASM suites and current strict checks, one compiler at a time.
set -euo pipefail
cd "$(dirname "$0")/../.."
export CARGO_BUILD_JOBS=1 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0
export PATH="$PWD/experiments/issue-138/tools:$PWD/experiments/issue-138/tools/wasm-bindgen-0.2.129-x86_64-unknown-linux-musl:$PATH"
prefix=experiments/issue-138/query-reservation
printf 'START %s\n' "$(date -u +%FT%TZ)"
cargo fmt --manifest-path rust/Cargo.toml --check > "$prefix-format.log" 2>&1
cargo fmt --manifest-path rust/wasm/Cargo.toml --check >> "$prefix-format.log" 2>&1
cargo clippy --manifest-path rust/Cargo.toml --all-features --all-targets -- -D warnings > "$prefix-clippy.log" 2>&1
cargo clippy --manifest-path rust/wasm/Cargo.toml --target wasm32-unknown-unknown --all-targets -- -D warnings > "$prefix-wasm-clippy.log" 2>&1
printf 'STRICT LINT PASS %s\n' "$(date -u +%FT%TZ)"
RUSTC_BOOTSTRAP=1 cargo llvm-cov test --manifest-path rust/Cargo.toml --all-features \
  --branch --json --output-path "$prefix-native-coverage.json" > "$prefix-native-full.log" 2>&1
cargo test --manifest-path rust/Cargo.toml --all-features --doc > "$prefix-doctests.log" 2>&1
RUSTDOCFLAGS='-D warnings' cargo doc --manifest-path rust/Cargo.toml --all-features --no-deps > "$prefix-docs.log" 2>&1
printf 'COMPLETE NATIVE PASS %s\n' "$(date -u +%FT%TZ)"
WASM_COVERAGE_EXTRA_RUSTFLAGS=-Clink-arg=--max-memory=268435456 \
  CARGO_LLVM_COV_BIN="$PWD/experiments/issue-138/tools/cargo-llvm-cov" \
  bash experiments/issue-138/wasm-coverage.sh > "$prefix-wasm-full.log" 2>&1
cp experiments/issue-138/wasm-coverage.json "$prefix-wasm-coverage.json"
printf 'COMPLETE ACTUAL WASM PASS %s\n' "$(date -u +%FT%TZ)"
cargo tree --manifest-path rust/Cargo.toml --no-default-features --edges normal > "$prefix-bare-dependencies.log" 2>&1
cargo tree --manifest-path rust/Cargo.toml --no-default-features --features causal-sessions --edges normal > "$prefix-selected-dependencies.log" 2>&1
if rg -q 'serde_json|rusqlite|sha2' "$prefix-bare-dependencies.log" || rg -q 'rusqlite|sha2' "$prefix-selected-dependencies.log"; then
  printf 'selected dependency boundary changed\n' >&2; exit 1
fi
npm run test:js --prefix js > "$prefix-repository.log" 2>&1
node csharp/scripts/check-file-size.mjs > "$prefix-source-size.log" 2>&1
node csharp/scripts/validate-changeset.mjs > "$prefix-changeset.log" 2>&1
PATH="$PWD/experiments/issue-138/tools/shellcheck-v0.11.0:$PATH" experiments/issue-138/tools/actionlint > "$prefix-workflows.log" 2>&1
experiments/issue-138/tools/shellcheck-v0.11.0/shellcheck experiments/issue-138/validate-query-reservations.sh >> "$prefix-workflows.log" 2>&1
git diff --check > "$prefix-whitespace.log" 2>&1
printf 'ALL PASS %s\n' "$(date -u +%FT%TZ)"
