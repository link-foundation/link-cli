#!/usr/bin/env bash
# Test-only audit: preserve the preceding production/integration reports.
set -euo pipefail
cd "$(dirname "$0")/../.."
export CARGO_BUILD_JOBS=1 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 DOTNET_GCHeapHardLimit=0x28000000
export PATH="$PWD/experiments/issue-138/tools/dotnet-10.0.403:$PWD/experiments/issue-138/tools/wasm-bindgen-0.2.129-x86_64-unknown-linux-musl:$PWD/experiments/issue-138/tools:$PATH"
prefix=experiments/issue-138/reclamation-contracts
printf 'START native %s\n' "$(date -u +%FT%TZ)"
cargo fmt --manifest-path rust/Cargo.toml --all -- --check
cargo clippy --manifest-path rust/Cargo.toml --all-features --all-targets -- -D warnings > "$prefix-clippy.log" 2>&1
RUSTC_BOOTSTRAP=1 cargo llvm-cov test --manifest-path rust/Cargo.toml --all-features --branch --json --output-path "$prefix-native-coverage.json" > "$prefix-native-full.log" 2>&1
cargo test --manifest-path rust/Cargo.toml --all-features --doc > "$prefix-doctests.log" 2>&1
printf 'PASS native; START csharp %s\n' "$(date -u +%FT%TZ)"
dotnet format csharp --no-restore --verify-no-changes > "$prefix-format-check.log" 2>&1
for mode in Debug Release; do
 dotnet test csharp --configuration "$mode" --disable-build-servers -m:1 --collect:'XPlat Code Coverage' --results-directory "$PWD/$prefix-csharp-$mode-results" > "$prefix-csharp-$mode-full.log" 2>&1
done
printf 'PASS csharp; START wasm %s\n' "$(date -u +%FT%TZ)"
cargo fmt --manifest-path rust/wasm/Cargo.toml -- --check
cargo clippy --manifest-path rust/wasm/Cargo.toml --target wasm32-unknown-unknown --all-targets -- -D warnings > "$prefix-wasm-clippy.log" 2>&1
WASM_COVERAGE_EXTRA_RUSTFLAGS=-Clink-arg=--max-memory=268435456 CARGO_LLVM_COV_BIN="$PWD/experiments/issue-138/tools/cargo-llvm-cov" bash experiments/issue-138/wasm-coverage.sh > "$prefix-wasm-full.log" 2>&1
cp experiments/issue-138/wasm-coverage.json "$prefix-wasm-coverage.json"
python3 experiments/issue-138/verify-wasm-memory-cap.py > "$prefix-wasm-memory-cap.log"
printf 'PASS wasm; START javascript %s\n' "$(date -u +%FT%TZ)"
(cd js/client && node --test --test-concurrency=1 --test-timeout=30000 --experimental-test-coverage --test-coverage-include='src/**' --test-coverage-lines=100 --test-coverage-branches=100 --test-coverage-functions=100 test/*.test.mjs && npm run typecheck) > "$prefix-js-full.log" 2>&1
node experiments/issue-138/verify-reclamation-lazy.mjs > "$prefix-lazy.log" 2>&1
bun test js/client/test/reclaiming*.test.mjs --timeout 30000 > "$prefix-bun.log" 2>&1
deno test --allow-all js/client/test/reclaiming*.test.mjs > "$prefix-deno.log" 2>&1
printf 'PASS javascript; START repository %s\n' "$(date -u +%FT%TZ)"
node csharp/scripts/check-file-size.mjs --lang all > "$prefix-file-size.log"
node csharp/scripts/validate-changeset.mjs > "$prefix-changeset.log"
for language in rust csharp js; do
 node .github/scripts/check-sqlite-optional.mjs "$language" > "$prefix-$language-optional.log" 2>&1
done
node --test .github/scripts/*.test.mjs js/test/*.test.mjs csharp/scripts/*.test.mjs > "$prefix-repository.log" 2>&1
experiments/issue-138/tools/shellcheck-v0.11.0/shellcheck experiments/issue-138/validate-reclamation-contracts.sh > "$prefix-shellcheck.log" 2>&1
experiments/issue-138/tools/actionlint -shellcheck="$PWD/experiments/issue-138/tools/shellcheck-v0.11.0/shellcheck" .github/workflows/js-client.yml .github/workflows/rust.yml .github/workflows/csharp.yml .github/workflows/wasm.yml > "$prefix-actionlint.log" 2>&1
git diff --check
git diff --cached --check
printf 'PASS repository; ALL PASSED %s\n' "$(date -u +%FT%TZ)"
