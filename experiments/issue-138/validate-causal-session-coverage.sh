#!/usr/bin/env bash
# Supplement the complete unchanged-production run without dropping denominators.
set -euo pipefail
cd "$(dirname "$0")/../.."
export CARGO_BUILD_JOBS=1 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 RUSTC_BOOTSTRAP=1
export PATH="$PWD/experiments/issue-138/tools:$PWD/experiments/issue-138/tools/wasm-bindgen-0.2.129-x86_64-unknown-linux-musl:$PATH"
prefix=experiments/issue-138/causal-session-coverage
printf 'START %s\n' "$(date -u +%FT%TZ)"
python3 experiments/issue-138/test-causal-raw-coverage.py > "$prefix-verifier-after.log" 2>&1
for mode in Debug Release; do
  report=$(find "experiments/issue-138/causal-session-$mode-results" -name coverage.json -type f)
  python3 experiments/issue-138/check-causal-raw-coverage.py "$report" --require-session > "$prefix-$mode-records.log" 2>&1
done
cargo fmt --manifest-path rust/Cargo.toml
cargo fmt --manifest-path rust/wasm/Cargo.toml
cargo clippy --manifest-path rust/Cargo.toml --all-features --all-targets -- -D warnings > "$prefix-native-clippy.log" 2>&1
cargo clippy --manifest-path rust/wasm/Cargo.toml --target wasm32-unknown-unknown --all-targets -- -D warnings > "$prefix-wasm-clippy.log" 2>&1
cargo llvm-cov test --manifest-path rust/Cargo.toml --all-features --test engine_causal_session_tests \
  --branch --no-report > "$prefix-native-tests.log" 2>&1
cargo llvm-cov report --manifest-path rust/Cargo.toml --branch --json \
  --output-path "$prefix-native.json" > "$prefix-native-report.log" 2>&1
printf 'NATIVE SUPPLEMENT PASS %s\n' "$(date -u +%FT%TZ)"
export RUSTUP_TOOLCHAIN=1.98.0 RUST_LOG=warn WASM_BINDGEN_TEST_TIMEOUT=30
export CARGO_TARGET_WASM32_UNKNOWN_UNKNOWN_RUNNER=wasm-bindgen-test-runner
export CARGO_TARGET_WASM32_UNKNOWN_UNKNOWN_RUSTFLAGS='-Cinstrument-coverage -Zcoverage-options=branch -Zno-profiler-runtime -Clink-args=--no-gc-sections --cfg=wasm_bindgen_unstable_test_coverage -Clink-arg=--max-memory=268435456'
cargo llvm-cov test --manifest-path rust/wasm/Cargo.toml --target wasm32-unknown-unknown \
  --test core causal_session --branch --no-report > "$prefix-wasm-tests.log" 2>&1
cargo llvm-cov test --manifest-path rust/wasm/Cargo.toml --target wasm32-unknown-unknown \
  --test causal_session_binding --branch --no-report > "$prefix-wasm-binding-tests.log" 2>&1
cargo llvm-cov report --manifest-path rust/wasm/Cargo.toml --target wasm32-unknown-unknown \
  --branch --json --output-path "$prefix-wasm.json" > "$prefix-wasm-report.log" 2>&1
printf 'WASM SUPPLEMENT PASS %s\n' "$(date -u +%FT%TZ)"
npm run test:js --prefix js > "$prefix-repository.log" 2>&1
PATH="$PWD/experiments/issue-138/tools/shellcheck-v0.11.0:$PATH" experiments/issue-138/tools/actionlint > "$prefix-workflows.log" 2>&1
experiments/issue-138/tools/shellcheck-v0.11.0/shellcheck experiments/issue-138/validate-causal-session{,-actual,-physical,-final,-coverage}.sh >> "$prefix-workflows.log" 2>&1
git diff --check > "$prefix-whitespace.log" 2>&1
printf 'ALL SUPPLEMENTS PASS %s\n' "$(date -u +%FT%TZ)"
