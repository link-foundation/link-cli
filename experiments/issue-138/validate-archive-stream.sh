#!/usr/bin/env bash
# Independent stages keep compilers sequential and preserve complete reports.
set -euo pipefail
cd "$(dirname "$0")/../.."
export CARGO_BUILD_JOBS=1 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0
export DOTNET_GCHeapHardLimit=0x28000000
export PATH="$PWD/experiments/issue-138/tools/wasm-bindgen-0.2.129-x86_64-unknown-linux-musl:$PWD/experiments/issue-138/tools:$PATH"
stage="${1:?choose native, wasm or repository}"
case "$stage" in
 native)
  cargo fmt --manifest-path rust/Cargo.toml --all -- --check
  cargo clippy --manifest-path rust/Cargo.toml --all-features --all-targets -- -D warnings > experiments/issue-138/archive-stream-clippy.log 2>&1
  RUSTC_BOOTSTRAP=1 cargo llvm-cov test --manifest-path rust/Cargo.toml --all-features --branch --json --output-path experiments/issue-138/archive-stream-native-coverage.json > experiments/issue-138/archive-stream-native-full.log 2>&1
  cargo test --manifest-path rust/Cargo.toml --all-features --doc > experiments/issue-138/archive-stream-doctests.log 2>&1
  ;;
 wasm)
  cargo fmt --manifest-path rust/wasm/Cargo.toml -- --check
  cargo clippy --manifest-path rust/wasm/Cargo.toml --target wasm32-unknown-unknown --all-targets -- -D warnings > experiments/issue-138/archive-stream-wasm-clippy.log 2>&1
  WASM_COVERAGE_EXTRA_RUSTFLAGS=-Clink-arg=--max-memory=268435456 CARGO_LLVM_COV_BIN="$PWD/experiments/issue-138/tools/cargo-llvm-cov" bash experiments/issue-138/wasm-coverage.sh > experiments/issue-138/archive-stream-wasm-full.log 2>&1
  cp experiments/issue-138/wasm-coverage.json experiments/issue-138/archive-stream-wasm-coverage.json
  python3 experiments/issue-138/verify-wasm-memory-cap.py > experiments/issue-138/archive-stream-wasm-memory-cap.log
  ;;
 repository)
  node csharp/scripts/validate-changeset.mjs > experiments/issue-138/archive-stream-changeset.log 2>&1
  node csharp/scripts/check-file-size.mjs --lang all > experiments/issue-138/archive-stream-file-size.log 2>&1
  for language in rust csharp js; do
   node .github/scripts/check-sqlite-optional.mjs "$language" > "experiments/issue-138/archive-stream-$language-optional.log" 2>&1
  done
  node --test .github/scripts/*.test.mjs js/test/*.test.mjs csharp/scripts/*.test.mjs > experiments/issue-138/archive-stream-repository.log 2>&1
  "$PWD/experiments/issue-138/tools/shellcheck-v0.11.0/shellcheck" experiments/issue-138/validate-archive-stream.sh examples/native-pages/run-interop.sh > experiments/issue-138/archive-stream-shellcheck.log 2>&1
  git diff --check
  ;;
 *) printf 'Unknown stage: %s\n' "$stage" >&2; exit 2 ;;
esac
printf 'Archive-stream %s stage: passed\n' "$stage"
