#!/usr/bin/env bash
# Public parser semantics; keep complete native coverage and bounded compilation.
set -euo pipefail
cd "$(dirname "$0")/../.."
export CARGO_BUILD_JOBS=1 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0
export PATH="$PWD/experiments/issue-138/tools:$PATH"
cargo fmt --manifest-path rust/Cargo.toml --all -- --check
cargo clippy --manifest-path rust/Cargo.toml --all-features --all-targets -- -D warnings > experiments/issue-138/arguments-clippy.log 2>&1
RUSTC_BOOTSTRAP=1 cargo llvm-cov test --manifest-path rust/Cargo.toml --all-features --branch --json --output-path experiments/issue-138/arguments-rust-coverage.json > experiments/issue-138/arguments-rust-full.log 2>&1
printf 'Complete native Rust with coverage: passed\n'
node csharp/scripts/validate-changeset.mjs > experiments/issue-138/arguments-changeset.log 2>&1
node csharp/scripts/check-file-size.mjs --lang all > experiments/issue-138/arguments-file-size.log 2>&1
node --test .github/scripts/*.test.mjs js/test/*.test.mjs csharp/scripts/*.test.mjs > experiments/issue-138/arguments-repository.log 2>&1
git diff --check
