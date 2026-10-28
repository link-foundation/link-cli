#!/usr/bin/env bash
# Run after validate-reclamation-admission.sh: the eligible-orphan fixture was
# strengthened during that initial run. Preserve complete production coverage.
set -euo pipefail
cd "$(dirname "$0")/../.."
export CARGO_BUILD_JOBS=1 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0
export DOTNET_ROOT="$PWD/experiments/issue-138/tools/dotnet-10.0.403"
export PATH="$DOTNET_ROOT:$PWD/experiments/issue-138/tools:$PWD/experiments/issue-138/tools/wasm-bindgen-0.2.129-x86_64-unknown-linux-musl:$PATH"
export DOTNET_GCHeapHardLimit=0x28000000
prefix=experiments/issue-138/reclamation-admission
printf 'START %s\n' "$(date -u +%FT%TZ)"
cargo fmt --manifest-path rust/Cargo.toml --all -- --check > "$prefix-final-format.log" 2>&1
cargo clippy --manifest-path rust/Cargo.toml --all-features --all-targets -- -D warnings > "$prefix-final-clippy.log" 2>&1
RUSTC_BOOTSTRAP=1 cargo llvm-cov test --no-clean --manifest-path rust/Cargo.toml --all-features --test engine_reclaiming_tests --branch --json --output-path "$prefix-native-coverage.json" > "$prefix-final-native-focused.log" 2>&1
printf 'NATIVE FINAL FIXTURE PASS %s\n' "$(date -u +%FT%TZ)"
WASM_COVERAGE_EXTRA_RUSTFLAGS=-Clink-arg=--max-memory=268435456 CARGO_LLVM_COV_BIN="$PWD/experiments/issue-138/tools/cargo-llvm-cov" bash experiments/issue-138/wasm-coverage.sh > "$prefix-wasm-full.log" 2>&1
cp experiments/issue-138/wasm-coverage.json "$prefix-wasm-coverage.json"
printf 'ACTUAL WASM FINAL FIXTURE PASS %s\n' "$(date -u +%FT%TZ)"
dotnet format csharp --no-restore --verify-no-changes > "$prefix-csharp-format.log" 2>&1
dotnet test csharp --configuration Debug --disable-build-servers -m:1 --collect:'XPlat Code Coverage' --results-directory "$PWD/$prefix-Debug-results" > "$prefix-csharp-Debug-full.log" 2>&1
printf 'CSHARP DEBUG FINAL FIXTURE PASS %s\n' "$(date -u +%FT%TZ)"
experiments/issue-138/tools/shellcheck-v0.11.0/shellcheck experiments/issue-138/{validate-admission-orphans,validate-admission-repository,validate-reclamation-admission,run-phase-context-before}.sh > "$prefix-final-shellcheck.log" 2>&1
git diff --check > "$prefix-whitespace.log" 2>&1
printf 'ALL PASS %s\n' "$(date -u +%FT%TZ)"
